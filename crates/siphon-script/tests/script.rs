//! Scripts on their engines: calls made and settled, values returned, errors thrown, steps spent, and a clock and
//! random numbers that are the host's, so a replay runs alike.

use siphon_script::{Call, Host, Limits, NoSuchCall, Script, Step, Thrown};

/// The time a test's scripts start at: 2026-11-05, 00:00 UTC.
const TIME: i64 = 1_793_836_800_000;

fn host<'a>(functions: &'a [&'a str], limits: Limits) -> Host<'a> {
    Host {
        namespace: "host",
        functions,
        limits,
        seed: 1,
    }
}

fn started(source: &str, functions: &[&str]) -> (Script, Step) {
    Script::start(source, &host(functions, Limits::default()), TIME)
}

fn call(id: u32, function: &str, arguments: &str) -> Call {
    Call {
        id,
        function: function.into(),
        arguments: arguments.into(),
    }
}

fn thrown(step: Step) -> Thrown {
    match step {
        Step::Threw(thrown) => thrown,
        other => panic!("it did not throw: {other:?}"),
    }
}

/// What `source` comes to, run with no functions.
fn ran(source: &str) -> Step {
    started(source, &[]).1
}

#[test]
fn a_script_calls_its_hosts_functions_awaits_them_logs_and_returns_a_value() {
    let source = r#"
const page = await host.coding.read({ path: "a.md" });
const [x, y] = await Promise.all([host.math.add({ a: 1, b: 2 }), host.math.add({ a: 3, b: 4 })]);
console.log("read", page.lines, { x });
return { first: page.lines[0], sum: x + y };"#;
    let (mut script, step) = started(source, &["coding.read", "math.add"]);
    assert_eq!(step, Step::Waiting(vec![call(1, "coding.read", r#"{"path":"a.md"}"#)]));
    script.settle(1, Ok(r##"{"lines":["# A"]}"##)).expect("waits on it");
    assert_eq!(
        script.step(TIME),
        Step::Waiting(vec![
            call(2, "math.add", r#"{"a":1,"b":2}"#),
            call(3, "math.add", r#"{"a":3,"b":4}"#)
        ])
    );
    script.settle(3, Ok("7")).expect("waits on it");
    // A step with a call still unsettled waits on, telling no call it told before.
    assert_eq!(script.step(TIME), Step::Waiting(vec![]));
    script.settle(2, Ok("3")).expect("waits on it");
    assert_eq!(script.step(TIME), Step::Returned(r##"{"first":"# A","sum":10}"##.into()));
    assert_eq!(script.output(), [r##"read ["# A"] {"x":3}"##]);
    assert_eq!(script.output(), Vec::<String>::new());
    assert_eq!(script.settle(2, Ok("3")), Err(NoSuchCall));
    // A script that came to an end stays as it ended.
    assert_eq!(script.step(TIME), Step::Returned(r##"{"first":"# A","sum":10}"##.into()));
}

#[test]
fn a_rejected_call_is_thrown_into_the_script_and_json_that_does_not_parse_rejects_it_too() {
    let (mut caught, _) = started(
        "try { await host.t.fail({}); } catch (e) { return 'caught ' + e.message; }",
        &["t.fail"],
    );
    caught.settle(1, Err("no such file")).expect("waits on it");
    assert_eq!(caught.step(TIME), Step::Returned(r#""caught no such file""#.into()));
    let (mut garbled, _) = started("try { await host.t.get(); } catch (e) { return e.name; }", &["t.get"]);
    garbled.settle(1, Ok("{not json")).expect("waits on it");
    assert_eq!(garbled.step(TIME), Step::Returned(r#""SyntaxError""#.into()));
    // A call with no argument, or one JSON cannot hold, is made with null.
    assert_eq!(
        started("await host.t.get(() => 1);", &["t.get"]).1,
        Step::Waiting(vec![call(1, "t.get", "null")])
    );
}

#[test]
fn a_script_throws_its_errors_and_its_steps_bound_it() {
    let error = thrown(ran("throw new TypeError('bad');"));
    assert_eq!((error.name.as_str(), error.message.as_str()), ("TypeError", "bad"));
    assert_eq!(thrown(ran("let = ;")).name, "SyntaxError");
    let other = thrown(ran("throw { code: 7 };"));
    assert_eq!((other.name.as_str(), other.message.as_str()), ("Thrown", r#"{"code":7}"#));
    let bounded = |source: &str| {
        Script::start(
            source,
            &host(
                &[],
                Limits {
                    steps_max: 100,
                    ..Limits::default()
                },
            ),
            TIME,
        )
        .1
    };
    assert_eq!(bounded("while (true) {}"), Step::OutOfSteps);
    assert_eq!(bounded("await 0; while (true) {}"), Step::OutOfSteps);
    assert_eq!(
        bounded("try { while (true) {} } catch { return 'caught'; }"),
        Step::OutOfSteps,
        "no script catches its bound"
    );
    assert!(thrown(ran("await new Promise(() => {});")).message.contains("waits on nothing"));
    assert_eq!(
        thrown(ran("return 10n;")).name,
        "TypeError",
        "a value JSON cannot hold is thrown as saying so threw"
    );
    assert_eq!(ran("return;"), Step::Returned("null".into()));
}

#[test]
fn a_scripts_memory_is_bounded() {
    let (_, step) = Script::start(
        "const kept = []; while (true) kept.push('x'.repeat(1024));",
        &host(
            &[],
            Limits {
                memory_max: 4 << 20,
                steps_max: u64::MAX,
            },
        ),
        TIME,
    );
    assert_eq!(thrown(step).name, "InternalError");
}

#[test]
fn scripts_run_each_on_its_engine_what_one_does_another_knows_not() {
    let (mut first, _) = started("const n = await host.t.get({}); return n * 2;", &["t.get"]);
    let (mut second, step) = started(
        "globalThis.mine = 1; const n = await host.t.get({}); return [n, typeof globalThis.mine];",
        &["t.get"],
    );
    assert_eq!(step, Step::Waiting(vec![call(1, "t.get", "{}")]));
    second.settle(1, Ok("7")).expect("waits on it");
    first.settle(1, Ok("21")).expect("waits on it");
    assert_eq!(second.step(TIME), Step::Returned(r#"[7,"number"]"#.into()));
    assert_eq!(first.step(TIME), Step::Returned("42".into()));
    assert_eq!(ran("return typeof globalThis.mine;"), Step::Returned(r#""undefined""#.into()));
}

#[test]
fn a_script_has_the_whole_language_quickjs_gives() {
    let source = r#"const made = [eval("1 + 1"), new Function("return 2")()];
return { made, kinds: [typeof Proxy, typeof WeakRef, typeof BigInt, typeof Reflect] };"#;
    assert_eq!(
        ran(source),
        Step::Returned(r#"{"made":[2,2],"kinds":["function","function","function","object"]}"#.into())
    );
}

#[test]
fn a_scripts_clock_tells_the_hosts_time_each_step_and_its_random_numbers_come_of_its_seed() {
    let source = r"
const before = [Date.now(), new Date().getTime(), typeof Date(), new Date(0).getTime(), new Date() instanceof Date];
await host.t.wait();
class Later extends Date {}
return [before, Date.now(), new Later() instanceof Date, Date.UTC(2026, 0, 1), Date.name, Date.length];";
    let (mut script, _) = started(source, &["t.wait"]);
    script.settle(1, Ok("null")).expect("waits on it");
    assert_eq!(
        script.step(TIME + 5),
        Step::Returned(format!(
            r#"[[{TIME},{TIME},"string",0,true],{},true,1767225600000,"Date",7]"#,
            TIME + 5
        ))
    );
    let random = |seed| {
        let host = Host {
            seed,
            ..host(&[], Limits::default())
        };
        Script::start("return [Math.random(), Math.random()];", &host, TIME).1
    };
    assert_eq!(random(1), random(1), "the same seed, the same numbers");
    assert_ne!(random(1), random(2));
    let Step::Returned(numbers) = random(7) else {
        panic!("it returned")
    };
    assert!(numbers.starts_with("[0."), "{numbers}");
}

#[test]
fn a_failures_stack_tells_the_scripts_places_its_lines_and_on_its_first_line_its_columns_as_on_any_other() {
    for (source, stack) in [
        ("null.x;", "    at <anonymous> (script.js:1:1)\n"),
        ("\nnull.x;", "    at <anonymous> (script.js:2:1)\n"),
        ("1;  null.x;", "    at <anonymous> (script.js:1:5)\n"),
        ("\n1;  null.x;", "    at <anonymous> (script.js:2:5)\n"),
        (
            "function f() { null.z; }\nf();",
            "    at f (script.js:1:16)\n    at <anonymous> (script.js:2:1)\n",
        ),
        (
            "\nfunction f() { null.z; }\nf();",
            "    at f (script.js:2:16)\n    at <anonymous> (script.js:3:1)\n",
        ),
    ] {
        assert_eq!(thrown(ran(source)).stack, stack, "{source:?}");
    }
}
