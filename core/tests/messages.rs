//! Messages as they go over the socket: a call read as its set's method by its name and params, an answer and an
//! event written as the client's TypeScript reads them.

use octoii::protocol::siphon::{Method, StopThread};
use octoii::protocol::{Answer, Call, ConnectionEvent, Error};
use octoii::threads::StopTarget;
use serde_json::json;

#[test]
fn a_call_is_read_as_its_set_s_method_by_its_name_and_params() {
    let call: Call = serde_json::from_value(json!({
        "id": "7",
        "method": "threads.stop",
        "params": { "thread": "t1", "target": { "kind": "turn", "id": "turn1" } }
    }))
    .expect("a call");
    assert_eq!(call.id, "7");
    let method = Method::read(&call.method, call.params).expect("a method of Octoii's");
    assert_eq!(
        method,
        Method::StopThread(StopThread {
            thread: "t1".into(),
            target: StopTarget::Turn { id: "turn1".into() },
        })
    );
    assert_eq!(method.name(), "threads.stop");
    assert!(Method::read("threads.fly", json!({})).is_err(), "no such method");
    assert!(
        Method::read("threads.stop", json!({ "thread": "t1" })).is_err(),
        "a stop requires an observed target"
    );
}

#[test]
fn an_answer_and_an_event_are_written_as_the_client_reads_them() {
    let answered = Answer::Result {
        id: "7".into(),
        result: json!({}),
    };
    assert_eq!(serde_json::to_value(&answered).expect("JSON"), json!({ "id": "7", "result": {} }));
    let refused = Answer::Error {
        id: None,
        error: Error {
            code: "bad_call".into(),
            message: "not JSON".into(),
        },
    };
    assert_eq!(
        serde_json::to_value(&refused).expect("JSON"),
        json!({ "id": null, "error": { "code": "bad_call", "message": "not JSON" } })
    );
    let lagged = ConnectionEvent::Lagged {
        subscription: "s1".into(),
        topic: json!({ "type": "threads" }),
    };
    assert_eq!(
        serde_json::to_value(&lagged).expect("JSON"),
        json!({ "event": "lagged", "subscription": "s1", "topic": { "type": "threads" } })
    );
}
