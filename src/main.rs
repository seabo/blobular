use sha1::{Digest, Sha1};
use std::io::Read;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use fastcdc::v2020;

/// Split a file into content-defined chunks.
pub fn chunk_file(buf: &[u8]) -> impl Iterator<Item = v2020::Chunk> + '_ {
    v2020::FastCDC::new(buf, 2048, 4096, 65535)
}

/// Command-line interface to `blobular`.
#[derive(Debug, Parser)]
#[command(name = "blobular")]
#[command(about = "Blob versioning", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Initialize a new blobular store in the current directory.
    Init,

    /// Add a blob to the store.
    #[command(arg_required_else_help = true)]
    Add {
        /// Path to add.
        #[arg(required = true)]
        path: Vec<PathBuf>,
    },

    /// Print a blob from the store.
    #[command(arg_required_else_help = true)]
    #[command(name = "cat-blob")]
    CatBlob {
        /// Hash of the blob to print.
        #[arg(required = true)]
        hash: String,
    },

    /// Print a file from the store.
    #[command(arg_required_else_help = true)]
    #[command(name = "cat-file")]
    CatFile {
        /// Hash of the file to print.
        #[arg(required = true)]
        hash: String,
    },
}

/// Find the `.blobular` directory. Uses the same logic as `git`.
///
/// We traverse upwards from the current directory, looking for a `.blobular`
/// directory. If we find one, we return it. If we reach the root directory
/// without finding one, we return an error.
fn find_dot_blobular() -> Result<PathBuf, ()> {
    // Get the current directory.
    let mut current_dir = std::env::current_dir().unwrap();

    // Loop until we find a `.blobular` directory or reach the root.
    loop {
        let dot_blobular = current_dir.join(".blobular");
        if dot_blobular.is_dir() {
            return Ok(dot_blobular);
        }

        // If we've reached the root, return an error.
        if !current_dir.pop() {
            return Err(());
        }
    }
}

/// Initialize a new blobular store in the current directory.
fn initialize_dot_blobular() {
    // Check that we are not already (nested) in a blobular repository.
    if let Ok(dot_blobular) = find_dot_blobular() {
        eprintln!("fatal: already inside a blobular repository");
        eprintln!(
            "note: the root of the blobular repository is: {}",
            dot_blobular.display()
        );
        std::process::exit(128);
    }

    // Create the `.blobular` directory.
    std::fs::create_dir(".blobular").unwrap();

    // Create the `objects` directory.
    std::fs::create_dir(".blobular/objects").unwrap();
}

/// Compute the SHA-1 hash of a file.
fn hash_file(path: &PathBuf) -> Result<String, std::io::Error> {
    // Open the file.
    let mut file = std::fs::File::open(path)?;

    // Create a hasher object.
    let mut hasher = Sha1::new();

    // Copy the entire file into the hasher.
    std::io::copy(&mut file, &mut hasher)?;

    // Compute the hash.
    let hash = hasher.finalize();

    // Convert the hash to a hex string.
    let hash = format!("{:x}", hash);

    Ok(hash)
}

/// Add a file to the blobular repository.
fn add_file_to_blobular_repo(path: PathBuf) {
    // Check that we are in a blobular repository.
    let dot_blobular = match find_dot_blobular() {
        Ok(dot_blobular) => dot_blobular,
        Err(()) => {
            eprintln!("fatal: not a blobular repository (or any of the parent directories)");
            eprintln!("run `blobular init` to create a new blobular repository");
            std::process::exit(128);
        }
    };

    // Check that the file exists.
    if !path.is_file() {
        eprintln!("fatal: pathspec '{:?}' did not match any files", path);
        std::process::exit(128);
    }

    // Check that the file is not empty.
    if path.metadata().unwrap().len() == 0 {
        eprintln!("fatal: empty file: {:?}", path);
        std::process::exit(128);
    }

    // Chunk the file into content-defined chunks.
    let file = std::fs::File::open(&path).unwrap();
    let file_bytes = std::io::BufReader::new(file)
        .bytes()
        .flatten()
        .collect::<Vec<_>>();
    let chunks = chunk_file(&file_bytes);

    // Compute the hash of the whole file.
    // NOTE: should build up the hash as we iterate the chunks of the file rather than
    // iterating through the same file multiple times.
    let blob_hash = hash_file(&path).unwrap();

    // Maintain a list of chunk hashes so we can write out the parent blob at the end.
    let mut chunk_hashes = Vec::new();

    // Store the chunks in the blobular repository.
    for chunk in chunks {
        let chunk_bytes = &file_bytes[chunk.offset..chunk.offset + chunk.length];

        // Calculate the SHA1 hash of the chunk.
        let mut hasher = Sha1::new();
        hasher.update(&chunk_bytes);
        let hash = hasher.finalize();
        let chunk_hash = format!("{:x}", hash);
        chunk_hashes.push(chunk_hash.clone());
        store_blob(&dot_blobular, &chunk_bytes, &chunk_hash);
    }

    // Build the parent blob.
    // It is formatted as:
    // blob <sub-blob-hash>
    // blob <sub-blob-hash>
    // etc.

    let mut parent_blob = Vec::new();
    for chunk_hash in chunk_hashes {
        parent_blob.extend_from_slice(b"blob ");
        parent_blob.extend_from_slice(chunk_hash.as_bytes());
        parent_blob.push(b'\n');
    }

    // Store the parent blob.
    store_blob(&dot_blobular, &parent_blob, &blob_hash);

    // Print the hash of the parent blob.
    println!("{}", blob_hash);
}

/// Store a blob in the blobular repository.
fn store_blob(dot_blobular: &PathBuf, blob: &[u8], blob_hash: &str) {
    // Compute the path to the object. The first two characters of the hash are
    // the directory name, and the rest of the hash is the file name.
    let object_path = dot_blobular
        .join("objects")
        .join(&blob_hash[..2])
        .join(&blob_hash[2..]);

    // If the object already exists, exit immediately with no error. This is just a no-op.
    if object_path.is_file() {
        return;
    }

    // Check that the object's directory exists, and create it if not.
    if let Some(parent) = object_path.parent() {
        if !parent.is_dir() {
            std::fs::create_dir_all(parent).unwrap();
        }
    }

    // Compress the blob with zlib using flate2 and write it to the object store.
    let object_file = std::fs::File::create(&object_path).unwrap();
    let mut encoder = flate2::write::ZlibEncoder::new(object_file, flate2::Compression::default());
    let mut blob_reader = std::io::BufReader::new(blob);
    std::io::copy(&mut blob_reader, &mut encoder).unwrap();
    encoder.finish().unwrap();
}

/// Retrieve the full hash from a prefix.
fn full_hash_from_prefix(prefix: &str, dot_blobular: &PathBuf) -> String {
    let hash = if prefix.len() == 40 {
        prefix.to_string()
    } else {
        if prefix.len() < 4 {
            eprintln!("fatal: ambiguous argument: {}", prefix);
            eprintln!("note: minimum length of a hash is 4 characters");
            std::process::exit(128);
        }

        // Find all objects that start with `hash`.
        let object_dir = dot_blobular.join("objects").join(&prefix[..2]);
        let mut matching_objects = Vec::new();
        for entry in std::fs::read_dir(object_dir).unwrap() {
            let entry = entry.unwrap();
            let entry = entry.file_name();
            let entry = entry.to_str().unwrap();
            if entry.starts_with(&prefix[2..]) {
                matching_objects.push(entry.to_string());
            }
        }

        // If there are no matching objects, exit with an error.
        if matching_objects.is_empty() {
            eprintln!("fatal: object not found: {}", prefix);
            std::process::exit(128);
        }

        // If there is more than one matching object, exit with an error.
        if matching_objects.len() > 1 {
            eprintln!("fatal: ambiguous argument: {}", prefix);
            eprintln!("note: the following objects start with the given hash:");
            for object in matching_objects {
                eprintln!("note:   {}", object);
            }
            std::process::exit(128);
        }

        // There is exactly one matching object. Use it. We print the full hash, including object dir.
        format!("{}{}", &prefix[..2], matching_objects[0])
    };
    hash
}

/// Print a blob from the store.
fn cat_blob_from_blobular_repo(hash: String) {
    // The hash can be a prefix, but we insist that it is at least 4 characters
    // long.
    if hash.len() < 4 {
        eprintln!("fatal: ambiguous argument: {}", hash);
        eprintln!("note: minimum length of a hash is 4 characters");
        std::process::exit(128);
    }

    // Check that we are in a blobular repository.
    let dot_blobular = match find_dot_blobular() {
        Ok(dot_blobular) => dot_blobular,
        Err(()) => {
            eprintln!("fatal: not a blobular repository (or any of the parent directories)");
            eprintln!("run `blobular init` to create a new blobular repository");
            std::process::exit(128);
        }
    };

    // `hash` can be a prefix of the full hash. Find the full hash.
    let hash = full_hash_from_prefix(&hash, &dot_blobular);

    // Compute the path to the object.
    let object_path = dot_blobular
        .join("objects")
        .join(&hash[..2])
        .join(&hash[2..]);

    // Check that the object exists.
    if !object_path.is_file() {
        eprintln!("fatal: object not found: {}", hash);
        std::process::exit(128);
    }

    // Decompress the object with zlib using flate2 and write it to stdout.
    let object_file = std::fs::File::open(&object_path).unwrap();
    let mut decoder = flate2::read::ZlibDecoder::new(object_file);
    std::io::copy(&mut decoder, &mut std::io::stdout()).unwrap();
}

/// Print a file from the store.
///
/// This prints the contents of the file. The blob hash that gets passed is expected to be
/// in the format of a parent blob, i.e. it is expected to be a blob that contains the hashes
/// of the chunks that make up the file. If not, this will fail.
fn cat_file_from_blobular_repo(hash: String) {
    // Check that we are in a blobular repository.
    let dot_blobular = match find_dot_blobular() {
        Ok(dot_blobular) => dot_blobular,
        Err(()) => {
            eprintln!("fatal: not a blobular repository (or any of the parent directories)");
            eprintln!("run `blobular init` to create a new blobular repository");
            std::process::exit(128);
        }
    };

    // `hash` can be a prefix of the full hash. Find the full hash.
    let hash = full_hash_from_prefix(&hash, &dot_blobular);

    // Compute the path to the object.
    let object_path = dot_blobular
        .join("objects")
        .join(&hash[..2])
        .join(&hash[2..]);

    // Check that the object exists.
    if !object_path.is_file() {
        eprintln!("fatal: object not found: {}", hash);
        std::process::exit(128);
    }

    // Decompress the object with zlib using flate2 and write it to stdout.
    let object_file = std::fs::File::open(&object_path).unwrap();
    let mut decoder = flate2::read::ZlibDecoder::new(object_file);
    let mut blob = Vec::new();
    decoder.read_to_end(&mut blob).unwrap();

    // Split the blob into lines.
    let parent_blob = String::from_utf8(blob).unwrap();
    let parent_blob: Vec<&str> = parent_blob.trim().split("\n").collect();

    // Verify the lines are of the form `blob <hash>` and extract the hash.
    let blob_hashes = parent_blob
        .iter()
        .map(|line| {
            let line = line.trim();
            if !line.starts_with("blob ") {
                eprintln!("fatal: invalid blob: {}", line);
                std::process::exit(128);
            }
            let hash = &line[5..];
            hash
        })
        .collect::<Vec<_>>();

    // For each line, print the blob.
    for line in blob_hashes {
        cat_blob_from_blobular_repo(line.to_string());
    }
}

fn main() {
    let args = Cli::parse();

    match args.command {
        Commands::Init => {
            initialize_dot_blobular();
        }
        Commands::Add { path } => {
            for path in path {
                add_file_to_blobular_repo(path);
            }
        }
        Commands::CatBlob { hash } => {
            cat_blob_from_blobular_repo(hash);
        }
        Commands::CatFile { hash } => {
            cat_file_from_blobular_repo(hash);
        }
    }
}
