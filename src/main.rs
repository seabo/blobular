use sha1::{Digest, Sha1};
use std::path::PathBuf;

use clap::{Parser, Subcommand};

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
    #[command(name = "cat-file")]
    CatFile {
        /// Hash of the blob to print.
        #[arg(required = true)]
        hash: String,
    },
}

/// Find the `.blobular` directory. Uses the same logic as `git`.
///
/// We traverse upwards from the current directory, looking for a `.blobular`
/// directory. If we find one, we return it. If we reach the root directory
/// without finding one, we return an error.
fn find_dot_blobular() -> PathBuf {
    // Get the current directory.
    let mut current_dir = std::env::current_dir().unwrap();

    // Loop until we find a `.blobular` directory or reach the root.
    loop {
        let dot_blobular = current_dir.join(".blobular");
        if dot_blobular.is_dir() {
            return dot_blobular;
        }

        // If we've reached the root, return an error.
        if !current_dir.pop() {
            // Exit with an error. But not a panic.
            eprintln!("fatal: not a blobular repository (or any of the parent directories)");
            eprintln!("run `blobular init` to create a new blobular repository");
            std::process::exit(128);
        }
    }
}

/// Initialize a new blobular store in the current directory.
fn initialize_dot_blobular() {
    // Check that we are not already (nested) in a blobular repository.
    let dot_blobular = find_dot_blobular();
    if dot_blobular.is_dir() {
        // The blobular repository is the directory that contains the .blobular
        let blobular_repo = dot_blobular.parent().unwrap();
        eprintln!(
            "fatal: already inside blobular repository {}",
            blobular_repo.display()
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
    let dot_blobular = find_dot_blobular();
    if !dot_blobular.is_dir() {
        eprintln!("fatal: not a blobular repository (or any of the parent directories)");
        eprintln!("run `blobular init` to create a new blobular repository");
        std::process::exit(128);
    }

    // Check that the file exists.
    if !path.is_file() {
        eprintln!("fatal: pathspec '{:?}' did not match any files", path);
        std::process::exit(128);
    }

    // Compute the hash of the file.
    let hash = hash_file(&path).unwrap();

    // Compute the path to the object. The first two characters of the hash are
    // the directory name, and the rest of the hash is the file name.
    let object_path = dot_blobular
        .join("objects")
        .join(&hash[..2])
        .join(&hash[2..]);

    // Check that the object doesn't already exist.
    if object_path.is_file() {
        eprintln!("fatal: object already exists: {}", hash);
        std::process::exit(128);
    }

    // Check that the object's directory exists, and create it if not.
    if let Some(parent) = object_path.parent() {
        if !parent.is_dir() {
            std::fs::create_dir_all(parent).unwrap();
        }
    }

    // Compress the file with zlib using flate2 and write it to the object store.
    let file = std::fs::File::open(&path).unwrap();
    let object_file = std::fs::File::create(&object_path).unwrap();
    let mut encoder = flate2::write::ZlibEncoder::new(object_file, flate2::Compression::default());
    std::io::copy(&mut std::io::BufReader::new(file), &mut encoder).unwrap();
    encoder.finish().unwrap();
}

/// Print a blob from the store.
fn cat_file_from_blobular_repo(hash: String) {
    // The hash can be a prefix, but we insist that it is at least 4 characters
    // long.
    if hash.len() < 4 {
        eprintln!("fatal: ambiguous argument: {}", hash);
        eprintln!("note: minimum length of a hash is 4 characters");
        std::process::exit(128);
    }

    // Check that we are in a blobular repository.
    let dot_blobular = find_dot_blobular();
    if !dot_blobular.is_dir() {
        eprintln!("fatal: not a blobular repository (or any of the parent directories)");
        eprintln!("run `blobular init` to create a new blobular repository");
        std::process::exit(128);
    }

    // `hash` can be a prefix of the full hash. Find the full hash.
    let hash = if hash.len() == 40 {
        hash
    } else {
        // Find all objects that start with `hash`.
        let object_dir = dot_blobular.join("objects").join(&hash[..2]);
        let mut matching_objects = Vec::new();
        for entry in std::fs::read_dir(object_dir).unwrap() {
            let entry = entry.unwrap();
            let entry = entry.file_name();
            let entry = entry.to_str().unwrap();
            if entry.starts_with(&hash[2..]) {
                matching_objects.push(entry.to_string());
            }
        }

        // If there are no matching objects, exit with an error.
        if matching_objects.is_empty() {
            eprintln!("fatal: object not found: {}", hash);
            std::process::exit(128);
        }

        // If there is more than one matching object, exit with an error.
        if matching_objects.len() > 1 {
            eprintln!("fatal: ambiguous argument: {}", hash);
            eprintln!("note: the following objects start with the given hash:");
            for object in matching_objects {
                eprintln!("note:   {}", object);
            }
            std::process::exit(128);
        }

        // There is exactly one matching object. Use it. We print the full hash, including object dir.
        format!("{}{}", &hash[..2], matching_objects[0])
    };

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
        Commands::CatFile { hash } => {
            cat_file_from_blobular_repo(hash);
        }
    }
}
