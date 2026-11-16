//! LuaRuntime(コア)の結合テスト。

use std::fmt;
use std::sync::{Arc, Mutex};

use lua_runtime::{Callback, CallbackError, Error, LuaRuntime, Value, ValueType};

type Log = Arc<Mutex<Vec<(String, Vec<Value>)>>>;

/// 呼び出しを `log` に記録して `ret` を返すコールバック。
fn recorder(log: &Log, tag: &str, ret: Value) -> Callback {
    let log = Arc::clone(log);
    let tag = tag.to_string();
    Arc::new(move |args: &[Value]| {
        log.lock().unwrap().push((tag.clone(), args.to_vec()));
        Ok(ret.clone())
    })
}

fn new_log() -> Log {
    Arc::new(Mutex::new(Vec::new()))
}

fn take(log: &Log) -> Vec<(String, Vec<Value>)> {
    std::mem::take(&mut *log.lock().unwrap())
}

#[test]
fn variable_add_set_get() {
    let mut rt = LuaRuntime::new();
    rt.add_variable("obj.x", ValueType::Float, Value::Float(100.0))
        .unwrap()
        .add_variable("obj.n", ValueType::Int, Value::Int(3))
        .unwrap()
        .add_variable("obj.inner.s", ValueType::Str, Value::Str("a".into()))
        .unwrap()
        .add_variable("flag", ValueType::Bool, Value::Bool(false))
        .unwrap();

    let x = rt
        .set_variable("obj.x", Value::Float(56.9))
        .unwrap()
        .run("")
        .unwrap()
        .get_variable("obj.x")
        .unwrap();
    assert_eq!(x, Value::Float(56.9));

    rt.run(
        "obj.x = obj.x * 2; obj.n = obj.n + 1; obj.inner.s = obj.inner.s .. 'b'; flag = not flag",
    )
    .unwrap();
    assert_eq!(rt.get_variable("obj.x").unwrap(), Value::Float(113.8));
    assert_eq!(rt.get_variable("obj.n").unwrap(), Value::Int(4));
    assert_eq!(
        rt.get_variable("obj.inner.s").unwrap(),
        Value::Str("ab".into())
    );
    assert_eq!(rt.get_variable("flag").unwrap(), Value::Bool(true));
}

#[test]
fn variable_type_errors() {
    let mut rt = LuaRuntime::new();
    rt.add_variable("n", ValueType::Int, Value::Int(1)).unwrap();
    // float は int を受け付けるが、int は float を受け付けない
    assert!(matches!(
        rt.set_variable("n", Value::Float(1.5)),
        Err(Error::Type(_))
    ));
    assert!(matches!(
        rt.set_variable("missing", Value::Int(1)),
        Err(Error::UnknownVariable(_))
    ));
    rt.add_variable("f", ValueType::Float, Value::Int(2))
        .unwrap();
    assert_eq!(rt.get_variable("f").unwrap(), Value::Float(2.0));

    // スクリプトが宣言と違う型を入れたら get で型エラー
    rt.run("n = 1.5").unwrap();
    assert!(matches!(rt.get_variable("n"), Err(Error::Type(_))));
}

#[test]
fn invalid_names() {
    let mut rt = LuaRuntime::new();
    rt.add_variable("a", ValueType::Int, Value::Int(1)).unwrap();
    let log = new_log();
    // 重複
    assert!(matches!(
        rt.add_variable("a", ValueType::Int, Value::Int(1)),
        Err(Error::InvalidName(_))
    ));
    // 途中がテーブルでない
    assert!(matches!(
        rt.add_function("a.f", recorder(&log, "f", Value::Nil), None, vec![]),
        Err(Error::InvalidName(_))
    ));
    // 空の要素
    assert!(matches!(
        rt.add_variable("b..c", ValueType::Int, Value::Int(1)),
        Err(Error::InvalidName(_))
    ));
}

#[test]
fn function_args_and_return() {
    let mut rt = LuaRuntime::new();
    let log = new_log();
    rt.add_variable("obj.out", ValueType::Float, Value::Float(0.0))
        .unwrap()
        .add_function(
            "obj.func",
            recorder(&log, "func", Value::Int(7)),
            Some(ValueType::Float),
            vec![ValueType::Str, ValueType::Int, ValueType::Bool],
        )
        .unwrap();

    // str は number も受け付ける(Lua の tostring と同じ規則)
    rt.run("obj.out = obj.func('hi', 3, true) + obj.func(12, 4.0, false)")
        .unwrap();
    assert_eq!(rt.get_variable("obj.out").unwrap(), Value::Float(14.0));
    assert_eq!(
        take(&log),
        vec![
            (
                "func".into(),
                vec![Value::Str("hi".into()), Value::Int(3), Value::Bool(true)]
            ),
            (
                "func".into(),
                vec![Value::Str("12".into()), Value::Int(4), Value::Bool(false)]
            ),
        ]
    );

    for script in [
        "obj.func('a', 1.5, true)",  // int に小数
        "obj.func('a', 1, 1)",       // bool に number
        "obj.func({}, 1, true)",     // str に table
        "obj.func('a', 1)",          // 引数不足
        "obj.func('a', 1, true, 2)", // 引数過多
    ] {
        let err = rt.run(script).unwrap_err();
        assert!(matches!(err, Error::Lua(_)), "{script}: {err}");
    }
    assert!(take(&log).is_empty());
}

#[test]
fn function_return_type_mismatch() {
    let mut rt = LuaRuntime::new();
    let log = new_log();
    rt.add_function(
        "f",
        recorder(&log, "f", Value::Str("x".into())),
        Some(ValueType::Int),
        vec![],
    )
    .unwrap();
    let err = rt.run("return f()").unwrap_err();
    assert!(err.to_string().contains("return value"), "{err}");
}

#[derive(Debug)]
struct MyError;

impl fmt::Display for MyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "my error")
    }
}

impl std::error::Error for MyError {}

#[test]
fn callback_error_is_returned_as_is() {
    let mut rt = LuaRuntime::new();
    let failing: Callback = Arc::new(|_: &[Value]| Err(Box::new(MyError) as CallbackError));
    rt.add_function("fail", failing, None, vec![]).unwrap();

    match rt.run("fail()").unwrap_err() {
        Error::Callback(e) => assert!(e.downcast_ref::<MyError>().is_some()),
        e => panic!("unexpected error: {e}"),
    }

    // pcall で握りつぶされたエラーの後に別のエラーが起きても、取り違えない
    match rt.run("pcall(fail); error('other')").unwrap_err() {
        Error::Lua(e) => assert!(e.to_string().contains("other"), "{e}"),
        e => panic!("unexpected error: {e}"),
    }
}

#[test]
fn realtime_procedure_runs_immediately() {
    let mut rt = LuaRuntime::new();
    let log = new_log();
    let order = Arc::clone(&log);
    let mark: Callback = Arc::new(move |_: &[Value]| {
        order.lock().unwrap().push(("mark".into(), vec![]));
        Ok(Value::Nil)
    });
    rt.add_procedure(
        "obj.proc",
        recorder(&log, "proc", Value::Float(1.0)),
        true,
        vec![ValueType::Str, ValueType::Int],
    )
    .unwrap()
    .add_function("mark", mark, None, vec![])
    .unwrap();

    // プロシージャは戻り値を返さない(nil)
    rt.run("assert(obj.proc('a', 1) == nil); mark()").unwrap();
    assert_eq!(
        take(&log),
        vec![
            ("proc".into(), vec![Value::Str("a".into()), Value::Int(1)]),
            ("mark".into(), vec![]),
        ]
    );
}

#[test]
fn deferred_procedures_run_after_script_in_call_order() {
    let mut rt = LuaRuntime::new();
    let log = new_log();
    let order = Arc::clone(&log);
    let mark: Callback = Arc::new(move |_: &[Value]| {
        order.lock().unwrap().push(("mark".into(), vec![]));
        Ok(Value::Nil)
    });
    rt.add_procedure(
        "obj.p1",
        recorder(&log, "p1", Value::Nil),
        false,
        vec![ValueType::Str, ValueType::Int, ValueType::Int],
    )
    .unwrap()
    .add_procedure(
        "p2",
        recorder(&log, "p2", Value::Nil),
        false,
        vec![ValueType::Float, ValueType::Bool],
    )
    .unwrap()
    .add_procedure("p0", recorder(&log, "p0", Value::Nil), false, vec![])
    .unwrap()
    .add_function("mark", mark, None, vec![])
    .unwrap();

    rt.run(
        r#"
        obj.p1("hello", 1, -2)
        p2(0.5, true)
        mark()
        p0()
        obj.p1(42, 3, 4)
        p2(-1, false)
        "#,
    )
    .unwrap();
    // mark(リアルタイム)が先、遅延プロシージャは呼び出し順に後から
    assert_eq!(
        take(&log),
        vec![
            ("mark".into(), vec![]),
            (
                "p1".into(),
                vec![Value::Str("hello".into()), Value::Int(1), Value::Int(-2)]
            ),
            ("p2".into(), vec![Value::Float(0.5), Value::Bool(true)]),
            ("p0".into(), vec![]),
            (
                "p1".into(),
                vec![Value::Str("42".into()), Value::Int(3), Value::Int(4)]
            ),
            ("p2".into(), vec![Value::Float(-1.0), Value::Bool(false)]),
        ]
    );

    // キューは run ごとに空になる
    rt.run("").unwrap();
    assert!(take(&log).is_empty());
}

#[test]
fn deferred_queue_grows() {
    let mut rt = LuaRuntime::new();
    let log = new_log();
    rt.add_procedure(
        "p",
        recorder(&log, "p", Value::Nil),
        false,
        vec![ValueType::Int, ValueType::Str],
    )
    .unwrap();
    // 初期容量(256 スロット)を何度も超える件数
    rt.run("for i = 1, 5000 do p(i, 's' .. i) end").unwrap();
    let calls = take(&log);
    assert_eq!(calls.len(), 5000);
    for (i, (_, args)) in calls.iter().enumerate() {
        let n = i as i64 + 1;
        assert_eq!(args, &vec![Value::Int(n), Value::Str(format!("s{n}"))]);
    }
}

#[test]
fn deferred_procedures_are_dropped_on_script_error() {
    let mut rt = LuaRuntime::new();
    let log = new_log();
    rt.add_procedure(
        "p",
        recorder(&log, "p", Value::Nil),
        false,
        vec![ValueType::Int],
    )
    .unwrap();
    assert!(rt.run("p(1); error('boom')").is_err());
    assert!(take(&log).is_empty());
    // 次の run に持ち越さない
    rt.run("p(2)").unwrap();
    assert_eq!(take(&log), vec![("p".into(), vec![Value::Int(2)])]);
}

#[test]
fn deferred_argument_errors() {
    let mut rt = LuaRuntime::new();
    let log = new_log();
    rt.add_procedure(
        "p",
        recorder(&log, "p", Value::Nil),
        false,
        vec![ValueType::Int, ValueType::Bool, ValueType::Float],
    )
    .unwrap();
    for script in [
        "p(1.5, true, 1)",
        "p(1, 1, 1)",
        "p(1, true, 'x')",
        "p(1, true)",
        "p(1, true, 1, 2)",
        "p(math.huge, true, 1)",
    ] {
        let err = rt.run(script).unwrap_err();
        assert!(matches!(err, Error::Lua(_)), "{script}: {err}");
    }
    assert!(take(&log).is_empty());
}

#[test]
fn deferred_callback_error_stops_flush() {
    let mut rt = LuaRuntime::new();
    let log = new_log();
    let failing: Callback = Arc::new(|_: &[Value]| Err(Box::new(MyError) as CallbackError));
    rt.add_procedure("bad", failing, false, vec![])
        .unwrap()
        .add_procedure(
            "p",
            recorder(&log, "p", Value::Nil),
            false,
            vec![ValueType::Int],
        )
        .unwrap();
    match rt.run("p(1); bad(); p(2)").unwrap_err() {
        Error::Callback(e) => assert!(e.downcast_ref::<MyError>().is_some()),
        e => panic!("unexpected error: {e}"),
    }
    assert_eq!(take(&log), vec![("p".into(), vec![Value::Int(1)])]);
}
