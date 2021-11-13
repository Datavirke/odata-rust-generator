#![feature(pattern)]

use clap::Parser;
use codegen::{Field, Function, Scope, Trait};
use indoc::indoc;
use odata_parser_rs::{Edmx, EntityType, NavigationProperty, Property, PropertyType, Schema};
use std::{
    collections::VecDeque,
    path::PathBuf,
    str::{pattern::Pattern, FromStr},
};

#[derive(Parser)]
#[clap(long_about = indoc! {"
    Command-line utility for generating Rust code from OData metadata.xml documents
"})]
struct Opts {
    #[clap(about = "Path to metadata.xml file to generate code from")]
    pub input_file: PathBuf,
    #[clap(
        long,
        about = "Don't derive Serialize and Deserialize traits to all structs"
    )]
    pub no_serde: bool,

    #[clap(
        long,
        about = "Don't coerce empty strings into None when deserializing into Option<String>"
    )]
    pub no_empty_string_is_null: bool,

    #[clap(
        long,
        about = "Don't produce OpenDataModel traits and implementations for run-time reflection"
    )]
    pub no_reflection: bool,

    #[clap(
        long,
        about = "Don't include NavigationProperties in the output structures. This makes deserializing $expand-ed properties impossible."
    )]
    pub no_expand: bool,

    #[clap(
        short,
        long,
        about = "Write output to file. If not specified, output will be printed to stdout"
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
            .map(|field| format!("(\"{}\", crate::OpenDataType::{})", field.0, field.1))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn lookup_entity_type(
    schema: &Schema,
    navigation_property: &NavigationProperty,
) -> Option<(String, String)> {
    let associations = &schema.associations;
    let namespace = format!("{}.", &schema.namespace);

    for association in associations.iter() {
        for end in &association.ends {
            if let Some(role) = &end.role {
                if role == &navigation_property.to_role {
                    if let Some(entity_type) = &end.entity_type {
                        return namespace
                            .strip_prefix_of(entity_type)
                            .map(String::from)
                            .map(|name| {
                                end.multiplicity
                                    .as_ref()
                                    .map(|multi| (name, multi.to_owned()))
                            })
                            .flatten();
                    }
                }
            }
        }
    }

    None
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
    root.raw(indoc! {"
            // Code automatically generated using https://github.com/Datavirke/odata-rust-generator
            // Any changes made to this file may be overwritten by future code generation runs!
        "});
    let mut contains_non_ascii = false;

    if !opts.no_empty_string_is_null {
        let mut function = Function::new("empty_string_as_none");
        function.attr("cfg(feature = \"serde\")");
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
        opendata_model.r#macro("#[cfg(feature = \"reflection\")]");
        opendata_model.vis("pub");
        opendata_model.new_fn("name").ret("&'static str");
        opendata_model
            .new_fn("fields")
            .ret("&'static [(&'static str, OpenDataType)]");
        opendata_model
            .new_fn("relations")
            .ret("&'static [(&'static str, &'static str)]");
        root.push_trait(opendata_model);

        let datatype = root.new_enum("OpenDataType").vis("pub");
        datatype.r#macro("#[cfg(feature = \"reflection\")]");

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

        if !opts.no_reflection && !schema.entities.is_empty() {
            let entity_types = head
                .new_fn("entity_types")
                .attr("cfg(feature = \"reflection\")")
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
            obj.r#macro("#[derive(Debug)]");

            if !opts.no_serde {
                obj.r#macro("#[cfg_attr(feature = \"serde\", derive(serde::Serialize, serde::Deserialize))]");
            }

            for property in &entity.properties {
                let typename = edm_type_to_rust_type(property);

                let mut field = if KEYWORDS.contains(&property.name.as_str()) {
                    Field::new(
                        &format!("pub r#{}", &property.name.to_lowercase()),
                        &typename,
                    )
                } else {
                    Field::new(&format!("pub {}", &property.name.to_lowercase()), &typename)
                };
                let mut annotations = Vec::new();

                if !opts.no_empty_string_is_null && typename == "Option<String>" {
                    annotations.push("#[cfg_attr(feature = \"serde\", serde(deserialize_with = \"crate::empty_string_as_none\"))]".to_string());
                };

                if property.name.chars().any(char::is_uppercase) {
                    annotations.push(format!(
                        "#[cfg_attr(feature = \"serde\", serde(rename = \"{}\"))]",
                        property.name
                    ));
                }
                field.annotation(annotations.iter().map(String::as_str).collect());

                obj.push_field(field);
            }

            if !opts.no_expand {
                for navigation_property in &entity.navigations {
                    let (typename, multiplicity) =
                        lookup_entity_type(schema, navigation_property).unwrap();

                    let typename = match multiplicity.as_str() {
                        "0..1" => format!("Option<Box<{}>>", typename),
                        _ => format!("Vec<{}>", typename),
                    };

                    let mut field = if KEYWORDS.contains(&navigation_property.name.as_str()) {
                        Field::new(
                            &format!("pub r#{}", &navigation_property.name.to_lowercase()),
                            &typename,
                        )
                    } else {
                        Field::new(
                            &format!("pub {}", &navigation_property.name.to_lowercase()),
                            &typename,
                        )
                    };
                    if navigation_property.name.chars().any(char::is_uppercase) {
                        field.annotation(vec![&format!(
                            "#[cfg_attr(feature = \"serde\", serde(rename = \"{}\", default))]",
                            navigation_property.name
                        )]);
                    }

                    obj.push_field(field);
                }
            }

            if !opts.no_reflection {
                let fields = entity_type_reflection(entity);
                let expansions = entity
                    .navigations
                    .iter()
                    .map(|nav| {
                        let (typename, _) = lookup_entity_type(schema, nav).unwrap();
                        format!("(\"{}\", \"{}\")", nav.name, typename)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                let opendata_model = head
                    .new_impl(&entity.name)
                    .impl_trait("crate::OpenDataModel");
                opendata_model.r#macro("#[cfg(feature = \"reflection\")]");
                opendata_model
                    .new_fn("name")
                    .ret("&'static str")
                    .line(format!("\"{}\"", &entity.name));
                opendata_model
                    .new_fn("fields")
                    .ret("&'static [(&'static str, crate::OpenDataType)]")
                    .line(fields);

                if !opts.no_expand {
                    opendata_model
                        .new_fn("relations")
                        .ret("&'static [(&'static str, &'static str)]")
                        .line(format!("&[{}]", expansions));
                }
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

    let output = root.to_string();
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
            no_expand: false,
            no_empty_string_is_null: false,
            no_reflection: false,
            output_file: None,
        })
    }
}
