use csv::{ReaderBuilder, WriterBuilder};
use forge_vm::value::Value;
use serde_json::{Number as JsonNumber, Value as JsonValue};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::rc::Rc;

/// データソースは `collect()` で vector に変換できる
pub trait PipelineSource<T> {
    fn collect(self) -> Result<Vec<T>, String>;
}

/// データシンクは dataset を受けて結果を返す
pub trait PipelineSink<T> {
    type Output;
    fn run(&self, dataset: Vec<T>) -> Result<Self::Output, String>;
}

#[derive(Debug, Clone)]
pub struct Group<K, V> {
    pub key: K,
    pub values: Vec<V>,
}

/// ListSource: `Vec<T>` をそのまま流す
#[derive(Debug, Clone)]
pub struct ListSource<T> {
    data: Vec<T>,
}

impl<T> ListSource<T> {
    pub fn new(data: Vec<T>) -> Self {
        Self { data }
    }
}

impl<T> PipelineSource<T> for ListSource<T> {
    fn collect(self) -> Result<Vec<T>, String> {
        Ok(self.data)
    }
}

#[derive(Debug, Clone)]
pub struct CsvSource {
    path: String,
}

impl CsvSource {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

impl PipelineSource<Value> for CsvSource {
    fn collect(self) -> Result<Vec<Value>, String> {
        read_csv_rows(&self.path)
    }
}

#[derive(Debug, Clone)]
pub struct JsonSource {
    path: String,
}

impl JsonSource {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

impl PipelineSource<Value> for JsonSource {
    fn collect(self) -> Result<Vec<Value>, String> {
        read_json_rows(&self.path)
    }
}

#[derive(Debug, Clone)]
pub struct CollectSink;

impl CollectSink {
    pub fn new() -> Self {
        CollectSink
    }
}

impl Default for CollectSink {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> PipelineSink<T> for CollectSink {
    type Output = Vec<T>;

    fn run(&self, dataset: Vec<T>) -> Result<Vec<T>, String> {
        Ok(dataset)
    }
}

#[derive(Debug, Clone)]
pub struct StdoutSink;

impl StdoutSink {
    pub fn new() -> Self {
        StdoutSink
    }
}

impl Default for StdoutSink {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: std::fmt::Debug> PipelineSink<T> for StdoutSink {
    type Output = ();

    fn run(&self, dataset: Vec<T>) -> Result<(), String> {
        for item in dataset {
            println!("{:?}", item);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CsvSink {
    path: String,
}

impl CsvSink {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

impl PipelineSink<Value> for CsvSink {
    type Output = ();

    fn run(&self, dataset: Vec<Value>) -> Result<(), String> {
        let rows = dataset_to_struct_rows(&dataset)?;
        write_csv_to_file(&self.path, &rows)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct JsonSink {
    path: String,
}

impl JsonSink {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

impl PipelineSink<Value> for JsonSink {
    type Output = ();

    fn run(&self, dataset: Vec<Value>) -> Result<(), String> {
        write_json_lines(&self.path, &dataset)?;
        Ok(())
    }
}

fn write_csv_to_file(path: &str, rows: &[HashMap<String, Value>]) -> Result<(), String> {
    if rows.is_empty() {
        File::create(path).map_err(|err| format!("failed to create '{}': {}", path, err))?;
        return Ok(());
    }

    let mut headers: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        for key in row.keys() {
            headers.insert(key.clone());
        }
    }

    let header_order: Vec<String> = headers.into_iter().collect();
    let mut writer = WriterBuilder::new()
        .has_headers(true)
        .from_path(path)
        .map_err(|err| format!("failed to open '{}': {}", path, err))?;
    writer
        .write_record(header_order.iter().map(|s| s.as_str()))
        .map_err(|err| format!("failed to write csv header: {}", err))?;
    for row in rows {
        let record: Vec<String> = header_order
            .iter()
            .map(|key| {
                row.get(key)
                    .map(|value| value_to_json_value(value).to_string())
                    .unwrap_or_default()
            })
            .collect();
        writer
            .write_record(record.iter().map(|s| s.as_str()))
            .map_err(|err| format!("failed to write csv record: {}", err))?;
    }
    writer
        .flush()
        .map_err(|err| format!("failed to flush '{}': {}", path, err))?;
    Ok(())
}

fn write_json_lines(path: &str, dataset: &[Value]) -> Result<(), String> {
    let mut file =
        File::create(path).map_err(|err| format!("failed to create '{}': {}", path, err))?;
    for value in dataset {
        let json = value_to_json_value(value);
        serde_json::to_writer(&mut file, &json)
            .map_err(|err| format!("failed to write json: {}", err))?;
        file.write_all(b"\n")
            .map_err(|err| format!("failed to write newline: {}", err))?;
    }
    Ok(())
}

fn value_to_json_value(value: &Value) -> JsonValue {
    match value {
        Value::Int(n) => JsonValue::Number(JsonNumber::from(*n)),
        Value::Float(f) => JsonNumber::from_f64(*f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        Value::String(s) => JsonValue::String(s.clone()),
        Value::Bool(b) => JsonValue::Bool(*b),
        Value::Unit => JsonValue::Null,
        Value::Option(Some(inner)) => value_to_json_value(inner),
        Value::Option(None) => JsonValue::Null,
        Value::Result(Ok(inner)) => value_to_json_value(inner),
        Value::Result(Err(err)) => JsonValue::String(err.clone()),
        Value::List(list) => {
            JsonValue::Array(list.borrow().iter().map(value_to_json_value).collect())
        }
        Value::Map(entries) => {
            let mut map = serde_json::Map::new();
            for (key, value) in entries {
                map.insert(key.to_string(), value_to_json_value(value));
            }
            JsonValue::Object(map)
        }
        Value::Struct { fields, .. } | Value::Typestate { fields, .. } => {
            let mut map = serde_json::Map::new();
            for (key, value) in fields.borrow().iter() {
                map.insert(key.clone(), value_to_json_value(value));
            }
            JsonValue::Object(map)
        }
        Value::Set(items) => JsonValue::Array(items.iter().map(value_to_json_value).collect()),
        _ => JsonValue::Null,
    }
}

fn read_csv_rows(path: &str) -> Result<Vec<Value>, String> {
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|err| format!("failed to open '{}': {}", path, err))?;
    let headers = reader
        .headers()
        .map_err(|err| format!("failed to read headers from '{}': {}", path, err))?
        .clone();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|err| format!("failed to read csv record: {}", err))?;
        let mut map: HashMap<String, Value> = HashMap::new();
        for (header, field) in headers.iter().zip(record.iter()) {
            map.insert(header.to_string(), parse_csv_field(field));
        }
        rows.push(row_struct("CsvRow", map));
    }
    Ok(rows)
}

fn read_json_rows(path: &str) -> Result<Vec<Value>, String> {
    let file = File::open(path).map_err(|err| format!("failed to open '{}': {}", path, err))?;
    let mut buf = String::new();
    BufReader::new(file)
        .read_to_string(&mut buf)
        .map_err(|err| format!("failed to read '{}': {}", path, err))?;
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if let Ok(json) = serde_json::from_str::<JsonValue>(trimmed) {
        if let JsonValue::Array(items) = json {
            return Ok(items.into_iter().map(json_to_value).collect());
        }
        return Ok(vec![json_to_value(json)]);
    }
    let mut rows = Vec::new();
    for line in buf.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let json: JsonValue = serde_json::from_str(line)
            .map_err(|err| format!("failed to parse json line: {}", err))?;
        rows.push(json_to_value(json));
    }
    Ok(rows)
}

fn parse_csv_field(field: &str) -> Value {
    if let Ok(n) = field.trim().parse::<i64>() {
        return Value::Int(n);
    }
    if let Ok(f) = field.trim().parse::<f64>() {
        return Value::Float(f);
    }
    Value::String(field.to_string())
}

fn row_struct(type_name: &str, fields: HashMap<String, Value>) -> Value {
    Value::Struct {
        type_name: type_name.to_string(),
        fields: Rc::new(RefCell::new(fields)),
    }
}

fn json_to_value(json: JsonValue) -> Value {
    match json {
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::String(n.to_string())
            }
        }
        JsonValue::String(s) => Value::String(s),
        JsonValue::Bool(b) => Value::Bool(b),
        JsonValue::Array(arr) => {
            let items = arr.into_iter().map(json_to_value).collect();
            Value::List(Rc::new(RefCell::new(items)))
        }
        JsonValue::Object(obj) => {
            let pairs: Vec<(Value, Value)> = obj
                .into_iter()
                .map(|(key, value)| (Value::String(key), json_to_value(value)))
                .collect();
            Value::Map(pairs)
        }
        JsonValue::Null => Value::Unit,
    }
}

fn dataset_to_struct_rows(dataset: &[Value]) -> Result<Vec<HashMap<String, Value>>, String> {
    let mut rows = Vec::new();
    for value in dataset {
        match value {
            Value::Struct { fields, .. } | Value::Typestate { fields, .. } => {
                rows.push(fields.borrow().clone());
            }
            other => {
                return Err(format!(
                    "pipeline csv sink expects struct rows, got {}",
                    other.type_name()
                ));
            }
        }
    }
    Ok(rows)
}
