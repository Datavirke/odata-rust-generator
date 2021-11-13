# odata-rust-generator
Generates Rust code that represents the models of an OpenData document


# Usage
Command-line utility for generating Rust code from OData metadata.xml documents

```
USAGE:
    odata-rust-generator [FLAGS] [OPTIONS] <input-file>

ARGS:
    <input-file>
            Path to metadata.xml file to generate code from

FLAGS:
    -h, --help
            Prints help information

        --no-empty-string-is-null
            Don't coerce empty strings into None when deserializing into Option<String>

        --no-expand
            Don't include NavigationProperties in the output structures. This makes deserializing
            $expand-ed properties impossible.

        --no-reflection
            Don't produce OpenDataModel traits and implementations for run-time reflection

        --no-serde
            Don't derive Serialize and Deserialize traits to all structs

    -V, --version
            Prints version information


OPTIONS:
    -o, --output-file <output-file>
            Write output to file. If not specified, output will be printed to stdout
```

# Example
Consume an OData 3.0 metadata file and generate a `odata.rs` file in the working directory, with all the Rust struct representations of the structures defined by the metadata file.
```bash
$ odata-rust-generator --output-file ./odata.rs metadata.xml
```