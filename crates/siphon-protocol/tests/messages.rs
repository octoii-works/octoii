//! Messages as they go over the socket: a call read by its method's name and params, an answer and an event written
//! as the client's TypeScript reads them.

use serde_json::json;
use siphon_protocol::{Answer, Call, Error, Event, Method, OfThread, Subscribe, Topic};

#[test]
fn a_call_is_read_by_its_method_s_name_and_params() {
    let call: Call = serde_json::from_value(json!({ "id": "7", "method": "threads.stop", "params": { "thread": "t1" } })).expect("a call");
    assert_eq!(call.id, "7");
    assert_eq!(call.method, Method::StopThread(OfThread { thread: "t1".into() }));
    assert_eq!(call.method.name(), "threads.stop");
    let subscribe: Call = serde_json::from_value(json!({
        "id": "8", "method": "subscribe", "params": { "topic": { "type": "threads" } }
    }))
    .expect("a call");
    assert_eq!(subscribe.method, Method::Subscribe(Subscribe { topic: Topic::Threads }));
    let unknown = serde_json::from_value::<Call>(json!({ "id": "9", "method": "threads.fly", "params": {} }));
    assert!(unknown.is_err(), "no such method");
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
    let lagged = Event::Lagged {
        subscription: "s1".into(),
        cursor: None,
    };
    assert_eq!(
        serde_json::to_value(&lagged).expect("JSON"),
        json!({ "event": "lagged", "subscription": "s1" })
    );
}
