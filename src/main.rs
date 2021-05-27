use clap::Clap;
use codegen::{Field, Function, Scope, Trait};
use indoc::indoc;
use odata_parser_rs::{Edmx, EntityType, Property, PropertyType};
use std::{collections::VecDeque, path::PathBuf, str::FromStr};

#[derive(Clap)]
#[clap(long_about = indoc! {"
    Command-line utility for generating Rust code from OData metadata.xml documents
"})]
struct Opts {
    #[clap(about = "path to metadata.xml file to generate code from")]
    pub input_file: PathBuf,
    #[clap(
        long,
        about = "don't derive Serialize and Deserialize traits to all structs"
    )]
    pub no_serde: bool,

    #[clap(
        long,
        about = "don't treat empty strings as nulls, when parsing from OpenData format"
    )]
    pub no_empty_string_is_null: bool,

    #[clap(
        long,
        about = "don't produce OpenDataModel traits and implementations for run-time reflection"
    )]
    pub no_reflection: bool,

    #[clap(
        short,
        long,
        about = "write output to file, writes to stdout if not specified"
    )]
    pub output_file: Option<PathBuf>,
}

const KEYWORDS: [&str; 1] = ["type"];

fn edm_type_to_rust_type(property: &Property) -> String {
    let inner = match property.inner {
        PropertyType::Binary { .. } => "Vec<u8>",
        PropertyType::Boolean { .. } => "bool",
        PropertyType::Byte { .. } => "u8",
        PropertyType::DateTime { .. } => "chrono::NaiveDateTime",
        PropertyType::DateTimeOffset { .. } => "std::time::Duration",
        PropertyType::Decimal { .. } => "f64",
        PropertyType::Double { .. } => "f64",
        PropertyType::Int16 { .. } => "i16",
        PropertyType::Int32 { .. } => "i32",
        PropertyType::String { .. } => "String",
    };

    if property.nullable {
        format!("Option<{}>", inner)
    } else {
        inner.to_string()
    }
}

fn entity_type_reflection(entity: &EntityType) -> String {
    let fields: Vec<(_, _)> = entity
        .properties
        .iter()
        .map(|property| {
            let typename = format!(
                "{} {{ nullable: {}, key: {} }}",
                match property.inner {
                    PropertyType::Binary { .. } => "Binary",
                    PropertyType::Boolean { .. } => "Boolean",
                    PropertyType::Byte { .. } => "Byte",
                    PropertyType::DateTime { .. } => "DateTime",
                    PropertyType::DateTimeOffset { .. } => "DateTimeOffset",
                    PropertyType::Decimal { .. } => "Decimal",
                    PropertyType::Double { .. } => "Double",
                    PropertyType::Int16 { .. } => "Int16",
                    PropertyType::Int32 { .. } => "Int32",
                    PropertyType::String { .. } => "String",
                },
                property.nullable,
                entity.key.property_ref.name == property.name
            );

            (property.name.clone(), typename)
        })
        .collect();

    format!(
        "&[{}]",
        fields
            .iter()
            .map(|field| format!("(\"{}\", OpenDataType::{})", field.0, field.1))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn print_structure(opts: Opts) {
    let source = std::fs::read_to_string(&opts.input_file).unwrap_or_else(|_| {
        panic!(
            "failed to read input metadata file at {}",
            opts.input_file.display()
        )
    });

    let project = Edmx::from_str(&source).expect("failed to parse metadata document");

    let mut root = Scope::new();
    root.raw(
        "// Code automatically generated using https://github.com/Datavirke/odata-rust-generator",
    );
    root.raw("// Any changes made to this file may be overwritten by future code generation runs!");
    let mut contains_non_ascii = false;

    if !opts.no_empty_string_is_null {
        let mut function = Function::new("empty_string_as_none");
        function.generic("'de").generic("D").generic("T");
        function.arg("de", "D");
        function.ret("Result<Option<T>, D::Error>");
        function
            .bound("T", "serde::Deserialize<'de>")
            .bound("D", "serde::Deserializer<'de>");
        function.line("let opt: Option<String> = serde::Deserialize::deserialize(de)?;");
        function.line("let opt = opt.as_deref();");
        function.line("match opt {");
        function.line("\tNone | Some(\"\") => Ok(None),");
        function.line("\tSome(s) => T::deserialize(serde::de::IntoDeserializer::into_deserializer(s)).map(Some),");
        function.line("}");
        root.push_fn(function);
    }

    if !opts.no_reflection {
        let mut opendata_model = Trait::new("OpenDataModel");
        opendata_model.vis("pub");
        opendata_model.new_fn("name").ret("&'static str");
        opendata_model
            .new_fn("fields")
            .ret("&'static [(&'static str, OpenDataType)]");
        root.push_trait(opendata_model);

        let datatype = root.new_enum("OpenDataType").vis("pub");
        datatype
            .new_variant("Binary")
            .named("nullable", "bool")
            .named("key", "bool");
        datatype
            .new_variant("Boolean")
            .named("nullable", "bool")
            .named("key", "bool");
        datatype
            .new_variant("Byte")
            .named("nullable", "bool")
            .named("key", "bool");
        datatype
            .new_variant("DateTime")
            .named("nullable", "bool")
            .named("key", "bool");
        datatype
            .new_variant("DateTimeOffset")
            .named("nullable", "bool")
            .named("key", "bool");
        datatype
            .new_variant("Decimal")
            .named("nullable", "bool")
            .named("key", "bool");
        datatype
            .new_variant("Double")
            .named("nullable", "bool")
            .named("key", "bool");
        datatype
            .new_variant("Int16")
            .named("nullable", "bool")
            .named("key", "bool");
        datatype
            .new_variant("Int32")
            .named("nullable", "bool")
            .named("key", "bool");
        datatype
            .new_variant("String")
            .named("nullable", "bool")
            .named("key", "bool");
    }

    for schema in &project.data_services.schemas {
        let mut path_segments: VecDeque<_> =
            schema.namespace.split('.').map(str::to_lowercase).collect();
        let mut head = root.get_or_new_module(&path_segments.pop_front().unwrap());
        head.vis("pub");

        for path_segment in path_segments {
            head = head.get_or_new_module(&path_segment);
            head.vis("pub");
            contains_non_ascii = contains_non_ascii || path_segment.is_ascii();
        }

        if !opts.no_serde && !schema.entities.is_empty() {
            head.import("serde", "Serialize");
            head.import("serde", "Deserialize");
        }

        if !opts.no_reflection && !schema.entities.is_empty() {
            head.import("crate", "OpenDataModel");
            head.import("crate", "OpenDataType");

            let entity_types = head
                .new_fn("entity_types")
                .vis("pub")
                .ret("&'static [(&'static str, &'static [(&'static str, crate::OpenDataType)])]")
                .line("&[");

            for entity in &schema.entities {
                entity_types.line(format!(
                    "\t(\"{}\", {}),",
                    entity.name,
                    entity_type_reflection(entity)
                ));
            }
            entity_types.line("]");
        }

        for entity in &schema.entities {
            let obj = head.scope().new_struct(&entity.name);
            obj.vis("pub");
            contains_non_ascii = contains_non_ascii || entity.name.is_ascii();

            if !opts.no_serde {
                obj.derive("Serialize").derive("Deserialize");
            }

            for property in &entity.properties {
                contains_non_ascii = contains_non_ascii || property.name.is_ascii();
                let typename = edm_type_to_rust_type(&property);

                let mut field = if KEYWORDS.contains(&property.name.as_str()) {
                    Field::new(&format!("pub r#{}", &property.name), &typename)
                } else {
                    Field::new(&format!("pub {}", &property.name), &typename)
                };

                if !opts.no_empty_string_is_null && typename == "Option<String>" {
                    field.annotation(vec![
                        "#[serde(deserialize_with = \"crate::empty_string_as_none\")]",
                    ]);
                };

                obj.push_field(field);
            }

            if !opts.no_reflection {
                let fields = entity_type_reflection(entity);

                let opendata_model = head.new_impl(&entity.name).impl_trait("OpenDataModel");
                opendata_model
                    .new_fn("name")
                    .ret("&'static str")
                    .line(format!("\"{}\"", &entity.name));
                opendata_model
                    .new_fn("fields")
                    .ret("&'static [(&'static str, OpenDataType)]")
                    .line(fields);
            }
        }

        if let Some(sets) = schema.entity_sets() {
            for set in sets {
                let mut path: Vec<_> = set.entity_type.split('.').map(str::to_lowercase).collect();
                path.pop();

                head.scope()
                    .import(&format!("crate::{}", path.join("::")), &set.name)
                    .vis("pub");
            }
        }
    }

    if let Some(default_schema) = project.default_schema() {
        root.import(&default_schema.namespace.to_lowercase(), "*")
            .vis("pub");
    }

    let output = format!(
        "{}{}",
        if contains_non_ascii {
            "#![feature(non_ascii_idents)]\n\n"
        } else {
            ""
        },
        root.to_string()
    );

    if let Some(output_file) = &opts.output_file {
        std::fs::write(&output_file, output).expect("failed to write output to file");
    } else {
        println!("{}", &output);
    }
}

fn main() {
    let opts = Opts::parse();

    print_structure(opts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generating_code_from_xml() {
        print_structure(Opts {
            input_file: PathBuf::from("tests/folketinget.xml"),
            no_serde: false,
            no_empty_string_is_null: false,
            no_reflection: false,
            output_file: None,
        })
    }
}
