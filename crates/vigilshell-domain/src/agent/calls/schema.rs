//! 内部标签与动作分支合并后，在每个完整请求分支关闭额外字段。
pub(super) fn close_tool_schema(schema: &mut schemars::Schema) {
    let Some(variants) = schema
        .as_object_mut()
        .and_then(|s| s.get_mut("oneOf"))
        .and_then(|v| v.as_array_mut())
    else {
        return;
    };
    for variant in variants {
        let Some(object) = variant.as_object_mut() else {
            continue;
        };
        let outer = object
            .get("properties")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let required = object
            .get("required")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if let Some(branches) = object.get_mut("oneOf").and_then(|v| v.as_array_mut()) {
            for branch in branches {
                let Some(branch) = branch.as_object_mut() else {
                    continue;
                };
                if let Some(properties) =
                    branch.get_mut("properties").and_then(|v| v.as_object_mut())
                {
                    properties.extend(outer.clone());
                }
                if let Some(values) = branch.get_mut("required").and_then(|v| v.as_array_mut()) {
                    for item in &required {
                        if !values.contains(item) {
                            values.push(item.clone());
                        }
                    }
                }
                branch.insert("additionalProperties".into(), false.into());
            }
        } else {
            object.insert("additionalProperties".into(), false.into());
        }
    }
}
