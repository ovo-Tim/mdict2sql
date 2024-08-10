# mdict2sql
Convert Mdict to SQLite in a high speed. Support multithreading now.

## Usage
```
Usage: mdict2sql [OPTIONS] <MDX_PATH> [OUTPUT_PATH]

Arguments:
  <MDX_PATH>     The path to the mdx file
  [OUTPUT_PATH]  The path to the output file(.db), defaults to <mdx_path>.db

Options:
  -t, --threads <THREADS>  The number of threads to use, defaults to your cpu cores [default: 16]
  -r, --remove-img-a       Remove <img> and <a> tags from the definition
  -h, --help               Print help
  -V, --version            Print version
```