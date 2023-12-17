# Blobular

Blobular is a blob storage system for versioning large files like datasets which may be incrementally updated. Unlike git, it does not store the entire file afresh for each version.

The interface is similar to `git` but Blobular uses content-defined chunking to split large files into smaller chunks. Chunks are stored in a content-addressed object store.

## Usage

```
Blob versioning

Usage: blobular <COMMAND>

Commands:
  init      Initialize a new blobular store in the current directory
  add       Add a blob to the store
  cat-blob  Print a blob from the store
  cat-file  Print a file from the store.
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

## Example

Initialize a new blobular store in the current directory:

```sh
> blobular init
```

Add a file to the store:

```sh
> blobular add src/main.rs
96d13f5cfe3f165cae26a3c97b1b0493b1a0bf3b
```

Inspect the blob we just added:

```sh
> blobular cat-blob 96d1
blob 4a67c6d9d739ea91fda83dd351465a77353bd030
blob 448396bce30fd31b813bc404720868e8d4e85247
blob 5f473d47a5adb538d116a774da7249ad6c235a04
blob a7ed923e2cc7cdea59ee50e55622de56d1c56197
```

The file was chunked into 4 smaller blobs. We can inspect the contents of each blob:

```sh
> blobular cat-blob 4a67
```

This prints out a subset of the file.

We can also inspect the file as a whole:

```sh
> blobular cat-file 96d1
```

This prints out the entire file.

Let's now add a comment to the top of the file.

```sh
> echo "// This is a comment" | cat - src/main.rs > src/main.rs.tmp && mv src/main.rs.tmp src/main.rs
```

We can now add the file again:

```sh
> blobular add src/main.rs
fabdbc6f11757b75ddcae0c4f4e83a66754337b1
```

Inspecting the blob we just added, we can see that only the first chunk has changed. The remaining chunks
are the same as before.

```sh
> blobular cat-blob fabd
blob ee7c046cbfee9d88228eb103b26c0e541db3cd04
blob 448396bce30fd31b813bc404720868e8d4e85247
blob 5f473d47a5adb538d116a774da7249ad6c235a04
blob a7ed923e2cc7cdea59ee50e55622de56d1c56197
```

## Further possibilities

## Deeper Merkle tree

The current data structure is basically a Merkle tree with a single level. We could extend this to have multiple levels so that very large files don't need to have huge lists of chunks at the top level, since even these blobs may be duplicative.

## File-format awareness

Some formats like .docx, image files etc. could be converted into a format that is more amenable to content-defined chunking. For example, a .docx file is really a zip archive of XML files. Blobular would do a much better job of deduplicating the data by first unzipping it into the raw XML files and then chunking those. This approach would require recording which conversion process was used to generate the chunks so that the file could be reconstructed.

## Blobular Cluster

Distributed blobular storage, similar to git's distributed model. Each node would have a local blobular store and would be able to push and pull from other nodes.

If, e.g., working with large datasets, one user might edit a dataset and store it in blobular. If another node wants to access the new version, it would only need to pull down a couple of small blobs and reconstruct the file locally from the new chunks and its existing local chunks from an older version. (I think this is broadly what `rsync` does, but the git-style interface and content-addressed object store feels much better.)
