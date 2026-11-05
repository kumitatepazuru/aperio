// params のレイアウト: ワード数・位置の計算と、ヘッダーへの埋め込み / 読み出しの往復。

use lua_gpu::codegen::layout::{decode_header, encode_header, Field, KernelLayout};
use lua_gpu::kernel::types::{ArrLen, GpuTy};

#[test]
fn words_and_offsets() {
    let k = KernelLayout {
        fields: vec![
            Field::U32,
            Field::F32,
            Field::Array(Box::new(Field::F32)),
            Field::Record(vec![
                ("gain".into(), Field::F32),
                ("offs".into(), Field::Array(Box::new(Field::F32))),
            ]),
            Field::Bool,
            Field::Records(17),
        ],
    };
    assert_eq!(k.offsets(), vec![0, 1, 2, 4, 7, 8]);
    assert_eq!(k.fixed_words(), 9);
}

#[test]
fn header_round_trip() {
    let kernels = vec![
        KernelLayout {
            fields: vec![
                Field::U32,
                Field::Array(Box::new(Field::Record(vec![
                    ("x".into(), Field::F32),
                    ("on".into(), Field::Bool),
                ]))),
                Field::Records(3),
            ],
        },
        KernelLayout { fields: vec![Field::U32] },
    ];
    let header = encode_header(&kernels);
    assert_eq!(header, "--[[lua_gpu v1 k0=u,a(r(x=f;on=b)),R3|k1=u]]");
    let lua = format!("{header} print('hi')\n");
    assert_eq!(decode_header(&lua).unwrap(), kernels);
    assert_eq!(decode_header("--[[lua_gpu v1 ]]").unwrap(), vec![]);
    assert!(decode_header("print(1)").is_err());
    assert!(decode_header("--[[lua_gpu v1 k0=x]]").is_err());
}

#[test]
fn fields_from_gpu_types() {
    let t = GpuTy::Record(vec![
        ("a".into(), GpuTy::Float),
        ("b".into(), GpuTy::Array(Box::new(GpuTy::Bool), ArrLen::Param)),
    ]);
    assert_eq!(
        Field::from_ty(&t),
        Some(Field::Record(vec![
            ("a".into(), Field::F32),
            ("b".into(), Field::Array(Box::new(Field::Bool))),
        ]))
    );
    assert_eq!(Field::from_ty(&GpuTy::Cpu), None);
}
