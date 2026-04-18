use serde::{Deserialize, Serialize};

const OP_SET_TEXT: i32 = 1;
const OP_SET_ATTR: i32 = 2;
const OP_ADD_LISTENER: i32 = 3;
const OP_REMOVE_LISTENER: i32 = 4;
const OP_SET_CLASS: i32 = 5;
const OP_INSERT_NODE: i32 = 6;
const OP_REMOVE_NODE: i32 = 7;
const OP_REPLACE_INNER: i32 = 8;
const OP_ATTACH: i32 = 9;

const EVENT_CLICK: i32 = 1;
const EVENT_INPUT: i32 = 2;
const EVENT_CHANGE: i32 = 3;
const EVENT_SUBMIT: i32 = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DomOp {
    SetText {
        target_id: String,
        value: String,
    },
    SetAttr {
        target_id: String,
        name: String,
        value: String,
    },
    SetClass {
        target_id: String,
        value: String,
    },
    InsertNode {
        target_id: String,
        html: String,
    },
    RemoveNode {
        target_id: String,
    },
    ReplaceInner {
        target_id: String,
        html: String,
    },
    Attach {
        target_id: String,
    },
    AddListener {
        target_id: String,
        event: String,
        handler_id: i32,
    },
    RemoveListener {
        target_id: String,
        event: String,
        handler_id: i32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    Click,
    Input,
    Change,
    Submit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRecord {
    pub kind: EventKind,
    pub target_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedDomOps {
    pub ops: Vec<i32>,
    pub strings: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedEventBuffer {
    pub events: Vec<i32>,
    pub strings: Vec<u8>,
}

pub fn serialize_dom_ops(items: &[DomOp]) -> EncodedDomOps {
    let mut ops = Vec::new();
    let mut strings = Vec::new();

    for item in items {
        match item {
            DomOp::SetText { target_id, value } => {
                ops.push(OP_SET_TEXT);
                push_string_ref(&mut ops, &mut strings, target_id);
                push_string_ref(&mut ops, &mut strings, value);
            }
            DomOp::SetAttr {
                target_id,
                name,
                value,
            } => {
                ops.push(OP_SET_ATTR);
                push_string_ref(&mut ops, &mut strings, target_id);
                push_string_ref(&mut ops, &mut strings, name);
                push_string_ref(&mut ops, &mut strings, value);
            }
            DomOp::SetClass { target_id, value } => {
                ops.push(OP_SET_CLASS);
                push_string_ref(&mut ops, &mut strings, target_id);
                push_string_ref(&mut ops, &mut strings, value);
            }
            DomOp::InsertNode { target_id, html } => {
                ops.push(OP_INSERT_NODE);
                push_string_ref(&mut ops, &mut strings, target_id);
                push_string_ref(&mut ops, &mut strings, html);
            }
            DomOp::RemoveNode { target_id } => {
                ops.push(OP_REMOVE_NODE);
                push_string_ref(&mut ops, &mut strings, target_id);
            }
            DomOp::ReplaceInner { target_id, html } => {
                ops.push(OP_REPLACE_INNER);
                push_string_ref(&mut ops, &mut strings, target_id);
                push_string_ref(&mut ops, &mut strings, html);
            }
            DomOp::Attach { target_id } => {
                ops.push(OP_ATTACH);
                push_string_ref(&mut ops, &mut strings, target_id);
            }
            DomOp::AddListener {
                target_id,
                event,
                handler_id,
            } => {
                ops.push(OP_ADD_LISTENER);
                push_string_ref(&mut ops, &mut strings, target_id);
                push_string_ref(&mut ops, &mut strings, event);
                ops.push(*handler_id);
            }
            DomOp::RemoveListener {
                target_id,
                event,
                handler_id,
            } => {
                ops.push(OP_REMOVE_LISTENER);
                push_string_ref(&mut ops, &mut strings, target_id);
                push_string_ref(&mut ops, &mut strings, event);
                ops.push(*handler_id);
            }
        }
    }

    EncodedDomOps { ops, strings }
}

pub fn deserialize_dom_ops(ops: &[i32], strings: &[u8]) -> Result<Vec<DomOp>, String> {
    let mut cursor = 0usize;
    let mut out = Vec::new();

    while cursor < ops.len() {
        let opcode = read_i32(ops, &mut cursor)?;
        match opcode {
            OP_SET_TEXT => {
                let target_id = read_string_ref(ops, strings, &mut cursor)?;
                let value = read_string_ref(ops, strings, &mut cursor)?;
                out.push(DomOp::SetText { target_id, value });
            }
            OP_SET_ATTR => {
                let target_id = read_string_ref(ops, strings, &mut cursor)?;
                let name = read_string_ref(ops, strings, &mut cursor)?;
                let value = read_string_ref(ops, strings, &mut cursor)?;
                out.push(DomOp::SetAttr {
                    target_id,
                    name,
                    value,
                });
            }
            OP_SET_CLASS => {
                let target_id = read_string_ref(ops, strings, &mut cursor)?;
                let value = read_string_ref(ops, strings, &mut cursor)?;
                out.push(DomOp::SetClass { target_id, value });
            }
            OP_INSERT_NODE => {
                let target_id = read_string_ref(ops, strings, &mut cursor)?;
                let html = read_string_ref(ops, strings, &mut cursor)?;
                out.push(DomOp::InsertNode { target_id, html });
            }
            OP_REMOVE_NODE => {
                let target_id = read_string_ref(ops, strings, &mut cursor)?;
                out.push(DomOp::RemoveNode { target_id });
            }
            OP_REPLACE_INNER => {
                let target_id = read_string_ref(ops, strings, &mut cursor)?;
                let html = read_string_ref(ops, strings, &mut cursor)?;
                out.push(DomOp::ReplaceInner { target_id, html });
            }
            OP_ATTACH => {
                let target_id = read_string_ref(ops, strings, &mut cursor)?;
                out.push(DomOp::Attach { target_id });
            }
            OP_ADD_LISTENER => {
                let target_id = read_string_ref(ops, strings, &mut cursor)?;
                let event = read_string_ref(ops, strings, &mut cursor)?;
                let handler_id = read_i32(ops, &mut cursor)?;
                out.push(DomOp::AddListener {
                    target_id,
                    event,
                    handler_id,
                });
            }
            OP_REMOVE_LISTENER => {
                let target_id = read_string_ref(ops, strings, &mut cursor)?;
                let event = read_string_ref(ops, strings, &mut cursor)?;
                let handler_id = read_i32(ops, &mut cursor)?;
                out.push(DomOp::RemoveListener {
                    target_id,
                    event,
                    handler_id,
                });
            }
            other => {
                return Err(format!("unknown DomOp opcode: {}", other));
            }
        }
    }

    Ok(out)
}

pub fn serialize_event_buffer(items: &[EventRecord]) -> EncodedEventBuffer {
    let mut events = Vec::new();
    let mut strings = Vec::new();

    for item in items {
        events.push(event_kind_code(&item.kind));
        push_string_ref(&mut events, &mut strings, &item.target_id);
    }

    EncodedEventBuffer { events, strings }
}

pub fn deserialize_event_buffer(events: &[i32], strings: &[u8]) -> Result<Vec<EventRecord>, String> {
    let mut cursor = 0usize;
    let mut out = Vec::new();

    while cursor < events.len() {
        let kind = decode_event_kind(read_i32(events, &mut cursor)?)?;
        let target_id = read_string_ref(events, strings, &mut cursor)?;
        out.push(EventRecord { kind, target_id });
    }

    Ok(out)
}

fn push_string_ref(buf: &mut Vec<i32>, strings: &mut Vec<u8>, value: &str) {
    let offset = strings.len() as i32;
    let bytes = value.as_bytes();
    strings.extend_from_slice(bytes);
    buf.push(offset);
    buf.push(bytes.len() as i32);
}

fn read_i32(buf: &[i32], cursor: &mut usize) -> Result<i32, String> {
    let value = buf
        .get(*cursor)
        .copied()
        .ok_or_else(|| "buffer ended unexpectedly".to_string())?;
    *cursor += 1;
    Ok(value)
}

fn read_string_ref(buf: &[i32], strings: &[u8], cursor: &mut usize) -> Result<String, String> {
    let offset = read_i32(buf, cursor)? as usize;
    let len = read_i32(buf, cursor)? as usize;
    let end = offset.saturating_add(len);
    let slice = strings
        .get(offset..end)
        .ok_or_else(|| format!("string ref out of bounds: {}..{}", offset, end))?;
    std::str::from_utf8(slice)
        .map(|value| value.to_string())
        .map_err(|err| format!("invalid utf-8 string ref: {}", err))
}

fn event_kind_code(kind: &EventKind) -> i32 {
    match kind {
        EventKind::Click => EVENT_CLICK,
        EventKind::Input => EVENT_INPUT,
        EventKind::Change => EVENT_CHANGE,
        EventKind::Submit => EVENT_SUBMIT,
    }
}

fn decode_event_kind(value: i32) -> Result<EventKind, String> {
    match value {
        EVENT_CLICK => Ok(EventKind::Click),
        EVENT_INPUT => Ok(EventKind::Input),
        EVENT_CHANGE => Ok(EventKind::Change),
        EVENT_SUBMIT => Ok(EventKind::Submit),
        other => Err(format!("unknown event kind: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_text_command() {
        let encoded = serialize_dom_ops(&[DomOp::SetText {
            target_id: "title".to_string(),
            value: "Hello".to_string(),
        }]);
        assert_eq!(encoded.ops, vec![OP_SET_TEXT, 0, 5, 5, 5]);
        assert_eq!(encoded.strings, b"titleHello");

        let decoded = deserialize_dom_ops(&encoded.ops, &encoded.strings).expect("decode");
        assert_eq!(
            decoded,
            vec![DomOp::SetText {
                target_id: "title".to_string(),
                value: "Hello".to_string(),
            }]
        );
    }

    #[test]
    fn test_add_listener_command() {
        let encoded = serialize_dom_ops(&[DomOp::AddListener {
            target_id: "btn".to_string(),
            event: "click".to_string(),
            handler_id: 7,
        }]);
        assert_eq!(encoded.ops, vec![OP_ADD_LISTENER, 0, 3, 3, 5, 7]);
        assert_eq!(encoded.strings, b"btnclick");

        let decoded = deserialize_dom_ops(&encoded.ops, &encoded.strings).expect("decode");
        assert_eq!(
            decoded,
            vec![DomOp::AddListener {
                target_id: "btn".to_string(),
                event: "click".to_string(),
                handler_id: 7,
            }]
        );
    }

    #[test]
    fn test_set_class_command() {
        let encoded = serialize_dom_ops(&[DomOp::SetClass {
            target_id: "card".to_string(),
            value: "active".to_string(),
        }]);
        assert_eq!(encoded.ops, vec![OP_SET_CLASS, 0, 4, 4, 6]);
        assert_eq!(encoded.strings, b"cardactive");

        let decoded = deserialize_dom_ops(&encoded.ops, &encoded.strings).expect("decode");
        assert_eq!(
            decoded,
            vec![DomOp::SetClass {
                target_id: "card".to_string(),
                value: "active".to_string(),
            }]
        );
    }

    #[test]
    fn test_insert_remove_node_commands() {
        let encoded = serialize_dom_ops(&[
            DomOp::InsertNode {
                target_id: "slot".to_string(),
                html: "<p>hi</p>".to_string(),
            },
            DomOp::RemoveNode {
                target_id: "slot".to_string(),
            },
        ]);
        let decoded = deserialize_dom_ops(&encoded.ops, &encoded.strings).expect("decode");
        assert_eq!(
            decoded,
            vec![
                DomOp::InsertNode {
                    target_id: "slot".to_string(),
                    html: "<p>hi</p>".to_string(),
                },
                DomOp::RemoveNode {
                    target_id: "slot".to_string(),
                }
            ]
        );
    }

    #[test]
    fn test_replace_inner_attach_commands() {
        let encoded = serialize_dom_ops(&[
            DomOp::ReplaceInner {
                target_id: "app".to_string(),
                html: "<p>ready</p>".to_string(),
            },
            DomOp::Attach {
                target_id: "app".to_string(),
            },
        ]);
        let decoded = deserialize_dom_ops(&encoded.ops, &encoded.strings).expect("decode");
        assert_eq!(
            decoded,
            vec![
                DomOp::ReplaceInner {
                    target_id: "app".to_string(),
                    html: "<p>ready</p>".to_string(),
                },
                DomOp::Attach {
                    target_id: "app".to_string(),
                }
            ]
        );
    }

    #[test]
    fn test_event_buffer_roundtrip() {
        let encoded = serialize_event_buffer(&[EventRecord {
            kind: EventKind::Click,
            target_id: "btn".to_string(),
        }]);
        assert_eq!(encoded.events, vec![EVENT_CLICK, 0, 3]);
        assert_eq!(encoded.strings, b"btn");

        let decoded = deserialize_event_buffer(&encoded.events, &encoded.strings).expect("decode");
        assert_eq!(
            decoded,
            vec![EventRecord {
                kind: EventKind::Click,
                target_id: "btn".to_string(),
            }]
        );
    }
}
