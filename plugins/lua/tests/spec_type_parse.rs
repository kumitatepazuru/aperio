// TypeSpec の記法(float / int / bool / string / any / T[N] / T[] / {k: T})のパースと表示。

use indexmap::IndexMap;
use lua_gpu::types::TypeSpec;

fn parse(s: &str) -> TypeSpec {
    s.parse().unwrap_or_else(|e| panic!("{s}: {e}"))
}

#[test]
fn parses_scalars_and_aliases() {
    assert_eq!(parse("float"), TypeSpec::Float);
    assert_eq!(parse("number"), TypeSpec::Float);
    assert_eq!(parse(" int "), TypeSpec::Int);
    assert_eq!(parse("bool"), TypeSpec::Bool);
    assert_eq!(parse("boolean"), TypeSpec::Bool);
    assert_eq!(parse("string"), TypeSpec::String);
    assert_eq!(parse("any"), TypeSpec::Any);
}

#[test]
fn parses_arrays_and_records() {
    assert_eq!(
        parse("float[4]"),
        TypeSpec::Array(Box::new(TypeSpec::Float), Some(4))
    );
    assert_eq!(parse("int[]"), TypeSpec::Array(Box::new(TypeSpec::Int), None));
    assert_eq!(
        parse("float[2][]"),
        TypeSpec::Array(
            Box::new(TypeSpec::Array(Box::new(TypeSpec::Float), Some(2))),
            None
        )
    );
    let mut fields = IndexMap::new();
    fields.insert("r".to_string(), TypeSpec::Float);
    fields.insert(
        "pts".to_string(),
        TypeSpec::Array(Box::new(TypeSpec::Int), None),
    );
    assert_eq!(parse("{r: float, pts: int[]}"), TypeSpec::Record(fields));
    assert_eq!(parse("{}"), TypeSpec::Record(IndexMap::new()));
}

#[test]
fn display_round_trips() {
    for s in ["float", "int[3]", "bool[]", "{a: float, b: {c: int[2]}}", "string", "any"] {
        assert_eq!(parse(s).to_string(), s);
        assert_eq!(parse(&parse(s).to_string()), parse(s));
    }
}

#[test]
fn rejects_invalid_types() {
    for s in [
        "", "double", "float[0]", "float[", "{a float}", "{a: float, a: int}", "float]", "{a: }",
    ] {
        assert!(s.parse::<TypeSpec>().is_err(), "`{s}` should be rejected");
    }
}

#[test]
fn gpu_representability() {
    assert!(parse("{a: float, b: int[]}").is_gpu_representable());
    assert!(!parse("{a: float, s: string}").is_gpu_representable());
    assert!(!parse("any[]").is_gpu_representable());
}
