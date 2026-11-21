//! 渠道级请求定制：额外请求头、参数覆盖、「有 tools 时去掉推理参数」。
//!
//! 这些都是纯函数，代理转发（proxy.rs）和渠道模型检测（channels.rs）共用，
//! 保证检测时发出的请求与真实转发一致。

use axum::http::{HeaderName, HeaderValue};
use serde_json::{Map, Value};

/// 单个渠道最多允许的额外请求头数量。
pub const MAX_EXTRA_HEADERS: usize = 32;
/// 单个请求头值的最大长度（字节）。
pub const MAX_HEADER_VALUE_LEN: usize = 2048;
/// 参数覆盖 JSON 序列化后的最大长度（字节）。
pub const MAX_OVERRIDE_LEN: usize = 16 * 1024;

/// 不允许通过「额外请求头」覆盖的头：鉴权、长度、编码与逐跳头由代理自己管理。
const FORBIDDEN_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "host",
    "content-length",
    "content-type",
    "content-encoding",
    "transfer-encoding",
    "connection",
    "keep-alive",
    "upgrade",
    "te",
    "trailer",
    "expect",
    "cookie",
];

/// 校验并规范化额外请求头。输入必须是 JSON 对象，值为字符串（数字/布尔会转成字符串）。
/// 返回规范化后的对象（头名小写）。
pub fn validate_extra_headers(value: &Value) -> Result<Value, String> {
    let obj = match value {
        Value::Null => return Ok(Value::Object(Map::new())),
        Value::Object(obj) => obj,
        _ => return Err("额外请求头必须是 JSON 对象，例如 {\"originator\":\"codex_cli_rs\"}".into()),
    };
    if obj.len() > MAX_EXTRA_HEADERS {
        return Err(format!("额外请求头最多 {MAX_EXTRA_HEADERS} 个"));
    }
    let mut out = Map::new();
    for (raw_name, raw_value) in obj {
        let name = raw_name.trim().to_ascii_lowercase();
        if name.is_empty() {
            return Err("请求头名称不能为空".into());
        }
        if HeaderName::from_bytes(name.as_bytes()).is_err() {
            return Err(format!("请求头名称不合法：{raw_name}"));
        }
        if FORBIDDEN_HEADERS.contains(&name.as_str()) || name.starts_with("proxy-") {
            return Err(format!("不允许自定义请求头：{raw_name}"));
        }
        let text = match raw_value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => return Err(format!("请求头 {raw_name} 的值必须是字符串")),
        };
        if text.len() > MAX_HEADER_VALUE_LEN {
            return Err(format!("请求头 {raw_name} 的值过长"));
        }
        if HeaderValue::from_str(&text).is_err() {
            return Err(format!("请求头 {raw_name} 的值含非法字符（不能有换行等控制字符）"));
        }
        if out.contains_key(&name) {
            return Err(format!("请求头重复：{raw_name}"));
        }
        out.insert(name, Value::String(text));
    }
    Ok(Value::Object(out))
}

/// 从数据库里存的 JSON 生成可直接附加的请求头列表。非法项静默跳过（入库时已校验）。
pub fn header_pairs(value: &Value) -> Vec<(HeaderName, HeaderValue)> {
    let Some(obj) = value.as_object() else {
        return Vec::new();
    };
    obj.iter()
        .filter_map(|(name, value)| {
            let name = HeaderName::from_bytes(name.as_bytes()).ok()?;
            if FORBIDDEN_HEADERS.contains(&name.as_str()) {
                return None;
            }
            let value = HeaderValue::from_str(value.as_str()?).ok()?;
            Some((name, value))
        })
        .collect()
}

/// 校验参数覆盖：必须是 JSON 对象；`model`、`stream` 不允许覆盖（会破坏路由与计费）。
pub fn validate_param_override(value: &Value) -> Result<Value, String> {
    let obj = match value {
        Value::Null => return Ok(Value::Object(Map::new())),
        Value::Object(obj) => obj,
        _ => return Err("参数覆盖必须是 JSON 对象，例如 {\"temperature\":0.7,\"reasoning_effort\":null}".into()),
    };
    for key in obj.keys() {
        let k = key.trim();
        if k.is_empty() {
            return Err("参数覆盖的字段名不能为空".into());
        }
        if k == "model" || k == "stream" {
            return Err(format!("参数覆盖不允许修改 {k} 字段"));
        }
        if k == STRIP_MESSAGE_FIELDS {
            let ok = obj[key]
                .as_array()
                .map(|a| a.iter().all(|v| v.as_str().map(|s| !s.trim().is_empty()).unwrap_or(false)))
                .unwrap_or(false);
            if !ok {
                return Err(format!("{STRIP_MESSAGE_FIELDS} 必须是字段名数组，例如 [\"reasoning_content\"]"));
            }
        }
    }
    let size = serde_json::to_string(value).map(|s| s.len()).unwrap_or(usize::MAX);
    if size > MAX_OVERRIDE_LEN {
        return Err("参数覆盖内容过大".into());
    }
    Ok(Value::Object(obj.clone()))
}

/// 参数覆盖里的特殊键：从 messages 每条消息中删除这些字段（例如上游不接受历史里的 reasoning_content）。
pub const STRIP_MESSAGE_FIELDS: &str = "__strip_message_fields";

/// 请求体里是否带了工具定义（chat 的 tools/functions，responses 的 tools）。
pub fn has_tools(body: &Map<String, Value>) -> bool {
    let non_empty = |v: Option<&Value>| match v {
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Null) | None => false,
        Some(_) => true,
    };
    non_empty(body.get("tools")) || non_empty(body.get("functions"))
}

/// 去掉推理参数：chat 的 `reasoning_effort`，responses 的 `reasoning`（整个对象）。
/// 返回是否真的删掉了什么。
#[allow(dead_code)]
pub fn strip_reasoning(body: &mut Map<String, Value>) -> bool {
    let a = body.remove("reasoning_effort").is_some();
    let b = body.remove("reasoning").is_some();
    a || b
}

/// 按渠道配置改写请求体：先「有 tools 去推理参数」，再应用参数覆盖。
/// `model` 与 `stream` 永远不会被覆盖。返回改动过的字段名列表（用于日志/调试）。
pub fn apply_channel_body_rules(
    body: &mut Value,
    strip_reasoning_with_tools: bool,
    param_override: &Value,
) -> Vec<String> {
    let mut changed = Vec::new();
    let Some(obj) = body.as_object_mut() else {
        return changed;
    };
    if strip_reasoning_with_tools && has_tools(obj) {
        if obj.remove("reasoning_effort").is_some() {
            changed.push("-reasoning_effort".to_string());
        }
        if obj.remove("reasoning").is_some() {
            changed.push("-reasoning".to_string());
        }
    }
    if let Some(over) = param_override.as_object() {
        if let Some(fields) = over.get(STRIP_MESSAGE_FIELDS).and_then(|v| v.as_array()) {
            let fields: Vec<&str> = fields.iter().filter_map(|v| v.as_str()).collect();
            let mut removed = 0usize;
            for list in ["messages", "input"] {
                if let Some(Value::Array(msgs)) = obj.get_mut(list) {
                    for msg in msgs.iter_mut() {
                        if let Some(m) = msg.as_object_mut() {
                            for f in &fields {
                                if m.remove(*f).is_some() {
                                    removed += 1;
                                }
                            }
                        }
                    }
                }
            }
            if removed > 0 {
                changed.push(format!("-messages[*].{}", fields.join("|")));
            }
        }
        for (key, value) in over {
            if key == "model" || key == "stream" || key == STRIP_MESSAGE_FIELDS {
                continue;
            }
            if value.is_null() {
                if obj.remove(key).is_some() {
                    changed.push(format!("-{key}"));
                }
            } else {
                obj.insert(key.clone(), value.clone());
                changed.push(format!("={key}"));
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strip_message_fields_removes_reasoning_content() {
        let over = validate_param_override(&json!({"__strip_message_fields": ["reasoning_content"]})).unwrap();
        let mut body = json!({"model":"m","messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"a","reasoning_content":"t"}]});
        let changed = apply_channel_body_rules(&mut body, false, &over);
        assert!(!changed.is_empty());
        assert!(body["messages"][1].get("reasoning_content").is_none());
        assert_eq!(body["messages"][1]["content"], "a");
        assert!(body.get("__strip_message_fields").is_none());
        assert!(validate_param_override(&json!({"__strip_message_fields": "x"})).is_err());
    }

    #[test]
    fn headers_accept_valid_object() {
        let v = validate_extra_headers(&json!({"Originator": "codex_cli_rs", "X-Num": 3})).unwrap();
        assert_eq!(v, json!({"originator": "codex_cli_rs", "x-num": "3"}));
        let pairs = header_pairs(&v);
        assert_eq!(pairs.len(), 2);
        assert!(pairs.iter().any(|(n, v)| n.as_str() == "originator" && v == "codex_cli_rs"));
    }

    #[test]
    fn headers_reject_bad_input() {
        assert!(validate_extra_headers(&json!(["a"])).is_err());
        assert!(validate_extra_headers(&json!("x")).is_err());
        assert!(validate_extra_headers(&json!({"bad name": "x"})).is_err());
        assert!(validate_extra_headers(&json!({"x-a": "line\nbreak"})).is_err());
        assert!(validate_extra_headers(&json!({"Authorization": "Bearer x"})).is_err());
        assert!(validate_extra_headers(&json!({"Host": "evil"})).is_err());
        assert!(validate_extra_headers(&json!({"proxy-foo": "1"})).is_err());
        assert!(validate_extra_headers(&json!({"x-a": {"n": 1}})).is_err());
        assert!(validate_extra_headers(&json!({"X-A": "1", "x-a": "2"})).is_err());
        assert_eq!(validate_extra_headers(&Value::Null).unwrap(), json!({}));
    }

    #[test]
    fn override_validation() {
        assert!(validate_param_override(&json!({"temperature": 0.2, "reasoning_effort": null})).is_ok());
        assert!(validate_param_override(&json!({"model": "x"})).is_err());
        assert!(validate_param_override(&json!({"stream": true})).is_err());
        assert!(validate_param_override(&json!([1])).is_err());
        assert_eq!(validate_param_override(&Value::Null).unwrap(), json!({}));
    }

    #[test]
    fn strip_reasoning_only_with_tools() {
        let mut chat = json!({"model": "gpt-6-astra", "reasoning_effort": "high",
            "tools": [{"type": "function", "function": {"name": "f", "parameters": {}}}]});
        let changed = apply_channel_body_rules(&mut chat, true, &json!({}));
        assert!(chat.get("reasoning_effort").is_none());
        assert_eq!(changed, vec!["-reasoning_effort"]);

        let mut responses = json!({"model": "m", "reasoning": {"effort": "high"}, "tools": [{"type": "function", "name": "f"}]});
        apply_channel_body_rules(&mut responses, true, &json!({}));
        assert!(responses.get("reasoning").is_none());

        // 没有 tools 时保留
        let mut plain = json!({"model": "m", "reasoning_effort": "high"});
        apply_channel_body_rules(&mut plain, true, &json!({}));
        assert_eq!(plain["reasoning_effort"], "high");

        // 空 tools 数组视为没有 tools
        let mut empty = json!({"model": "m", "reasoning_effort": "high", "tools": []});
        apply_channel_body_rules(&mut empty, true, &json!({}));
        assert_eq!(empty["reasoning_effort"], "high");

        // 开关关闭时不动
        let mut off = json!({"model": "m", "reasoning_effort": "high", "tools": [{"type": "function"}]});
        apply_channel_body_rules(&mut off, false, &json!({}));
        assert_eq!(off["reasoning_effort"], "high");

        // 旧式 functions 也算
        let mut legacy = json!({"model": "m", "reasoning_effort": "low", "functions": [{"name": "f"}]});
        apply_channel_body_rules(&mut legacy, true, &json!({}));
        assert!(legacy.get("reasoning_effort").is_none());
    }

    #[test]
    fn param_override_sets_and_deletes_but_never_model_or_stream() {
        let mut body = json!({"model": "a", "stream": true, "temperature": 1, "top_p": 0.5});
        let over = json!({"temperature": 0.1, "top_p": null, "max_tokens": 64, "model": "evil", "stream": false});
        apply_channel_body_rules(&mut body, false, &over);
        assert_eq!(body["model"], "a");
        assert_eq!(body["stream"], true);
        assert_eq!(body["temperature"], 0.1);
        assert!(body.get("top_p").is_none());
        assert_eq!(body["max_tokens"], 64);
    }

    #[test]
    fn has_tools_detection() {
        assert!(has_tools(json!({"tools": [{}]}).as_object().unwrap()));
        assert!(!has_tools(json!({"tools": []}).as_object().unwrap()));
        assert!(!has_tools(json!({"tools": null}).as_object().unwrap()));
        assert!(!has_tools(json!({}).as_object().unwrap()));
        let mut m = json!({"reasoning": {}, "reasoning_effort": "x"}).as_object().unwrap().clone();
        assert!(strip_reasoning(&mut m));
        assert!(m.is_empty());
    }
}
