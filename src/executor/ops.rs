use std::sync::Arc;
use super::executor::ExecutionError;
use crate::data::SerializableValue;
use crate::dsl::ast::{BinaryOp, Value};

pub(super) fn truth(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Number(n) => *n != 0,
        Value::Float(n) => *n != 0.0 && !n.is_nan(),
        Value::String(s) => !s.is_empty(),
        Value::List(items) => !items.is_empty(),
        Value::Bytes(bytes) => !bytes.is_empty(),
        Value::Record { fields, .. } | Value::Object(fields) => !fields.is_empty(),
    }
}

pub(super) fn binary(op: &BinaryOp, left: Value, right: Value) -> Result<Value, ExecutionError> {
    use Value::{Bool, Number, String as Text};
    match op {
        BinaryOp::Add => match (left, right) {
            (Text(a), Text(b)) => Ok(Text(format!("{a}{b}"))),
            (left, right) => arith(left, right, |a, b| a + b, |a, b| a + b),
        },
        BinaryOp::Sub => arith(left, right, |a, b| a - b, |a, b| a - b),
        BinaryOp::Mul => arith(left, right, |a, b| a * b, |a, b| a * b),
        BinaryOp::Div => match (left, right) {
            (Number(a), Number(b)) => {
                if b == 0 {
                    return Err(ExecutionError::new(4013, "除以零".into()));
                }
                Ok(Number(a / b))
            }
            (left, right) => {
                let value = arith(left, right, |_, _| 0, |a, b| a / b)?;
                if let Value::Float(n) = value {
                    if !n.is_finite() {
                        return Err(ExecutionError::new(4013, "除以零".into()));
                    }
                }
                Ok(value)
            }
        },
        BinaryOp::Eq => Ok(Bool(left == right)),
        BinaryOp::Ne => Ok(Bool(left != right)),
        BinaryOp::And => Ok(Bool(truth(&left) && truth(&right))),
        BinaryOp::Or => Ok(Bool(truth(&left) || truth(&right))),
        BinaryOp::Gt | BinaryOp::Lt | BinaryOp::Ge | BinaryOp::Le => cmp_ord(op, left, right),
    }
}

pub(super) fn to_serial(value: &Value) -> Result<SerializableValue, ExecutionError> {
    match value {
        Value::String(s) => Ok(SerializableValue::String(s.clone())),
        Value::Number(n) => Ok(SerializableValue::Int(*n)),
        Value::Float(n) => Ok(SerializableValue::Float(*n)),
        Value::Bool(b) => Ok(SerializableValue::Bool(*b)),
        Value::List(items) => Ok(SerializableValue::Array(
            items.iter().map(to_serial).collect::<Result<Vec<_>, _>>()?,
        )),
        Value::Bytes(_) => Err(ExecutionError::new(4012, "字节不能直接序列化".into())),
        Value::Record { .. } => Err(ExecutionError::new(4012, "记录不能直接序列化".into())),
        Value::Object(fields) => {
            let mut map = std::collections::HashMap::new();
            for (key, value) in fields.iter() {
                map.insert(key.clone(), to_serial(value)?);
            }
            Ok(SerializableValue::Object(map))
        }
    }
}

pub(super) fn from_serial(value: &SerializableValue) -> Result<Value, ExecutionError> {
    match value {
        SerializableValue::Bool(b) => Ok(Value::Bool(*b)),
        SerializableValue::Int(n) => Ok(Value::Number(*n)),
        SerializableValue::Float(n) => Ok(Value::Float(*n)),
        SerializableValue::String(s) => Ok(Value::String(s.clone())),
        SerializableValue::Array(items) => Ok(Value::List(Arc::new(items.iter().map(from_serial).collect::<Result<Vec<_>, _>>()?))),
        SerializableValue::Null => Err(ExecutionError::new(4012, "空值不能进入语言值".into())),
        SerializableValue::Object(map) => Ok(Value::Object(Arc::new(map.iter().map(|(k, v)| Ok((k.clone(), from_serial(v)?))).collect::<Result<Vec<_>, _>>()?))),
    }
}

fn arith(left: Value, right: Value, on_int: impl Fn(i64, i64) -> i64, on_float: impl Fn(f64, f64) -> f64) -> Result<Value, ExecutionError> {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => Ok(Value::Number(on_int(a, b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(on_float(a, b))),
        (Value::Number(a), Value::Float(b)) => Ok(Value::Float(on_float(a as f64, b))),
        (Value::Float(a), Value::Number(b)) => Ok(Value::Float(on_float(a, b as f64))),
        _ => Err(ExecutionError::new(4012, "运算两侧类型不同".into())),
    }
}

fn cmp_ord(op: &BinaryOp, left: Value, right: Value) -> Result<Value, ExecutionError> {
    let ord = match (left, right) {
        (Value::Number(a), Value::Number(b)) => a.cmp(&b),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(&b).ok_or_else(|| ExecutionError::new(4012, "小数不能比较".into()))?,
        (Value::Number(a), Value::Float(b)) => (a as f64).partial_cmp(&b).ok_or_else(|| ExecutionError::new(4012, "小数不能比较".into()))?,
        (Value::Float(a), Value::Number(b)) => a.partial_cmp(&(b as f64)).ok_or_else(|| ExecutionError::new(4012, "小数不能比较".into()))?,
        (Value::String(a), Value::String(b)) => a.cmp(&b),
        _ => return Err(ExecutionError::new(4012, "比较的类型不同".into())),
    };
    let ok = match op {
        BinaryOp::Gt => ord.is_gt(),
        BinaryOp::Lt => ord.is_lt(),
        BinaryOp::Ge => ord.is_ge(),
        BinaryOp::Le => ord.is_le(),
        _ => false,
    };
    Ok(Value::Bool(ok))
}
