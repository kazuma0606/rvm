//! ForgeScript bytecode definitions

use rvm_core::Value;

/// Bytecode instruction
#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    LoadConst(usize),
    LoadGlobal(usize),
    StoreGlobal(usize),
    Add,
    Sub,
    Mul,
    Div,
    Call(u8),
    Pop,
    Return,
}

/// Constant pool for storing literals
#[derive(Debug, Clone, PartialEq)]
pub struct ConstantPool {
    constants: Vec<Value>,
}

impl ConstantPool {
    pub fn new() -> Self {
        Self {
            constants: Vec::new(),
        }
    }

    pub fn add(&mut self, value: Value) -> usize {
        let index = self.constants.len();
        self.constants.push(value);
        index
    }

    pub fn get(&self, index: usize) -> Option<&Value> {
        self.constants.get(index)
    }

    pub fn len(&self) -> usize {
        self.constants.len()
    }

    pub fn is_empty(&self) -> bool {
        self.constants.is_empty()
    }
}

impl Default for ConstantPool {
    fn default() -> Self {
        Self::new()
    }
}

/// String pool for storing string names (identifiers, etc.)
#[derive(Debug, Clone, PartialEq)]
pub struct StringPool {
    strings: Vec<String>,
}

impl StringPool {
    pub fn new() -> Self {
        Self {
            strings: Vec::new(),
        }
    }

    pub fn add(&mut self, s: String) -> usize {
        if let Some(index) = self.strings.iter().position(|x| x == &s) {
            return index;
        }
        let index = self.strings.len();
        self.strings.push(s);
        index
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        self.strings.get(index).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

impl Default for StringPool {
    fn default() -> Self {
        Self::new()
    }
}

/// A chunk of bytecode with associated constant and string pools
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub instructions: Vec<Instruction>,
    pub constants: ConstantPool,
    pub strings: StringPool,
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            constants: ConstantPool::new(),
            strings: StringPool::new(),
        }
    }

    pub fn add_instruction(&mut self, instruction: Instruction) {
        self.instructions.push(instruction);
    }

    pub fn add_constant(&mut self, value: Value) -> usize {
        self.constants.add(value)
    }

    pub fn add_string(&mut self, s: String) -> usize {
        self.strings.add(s)
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

impl Chunk {
    /// Disassemble the chunk for debugging
    pub fn disassemble(&self, name: &str) -> String {
        let mut output = String::new();
        output.push_str(&format!("=== {} ===\n", name));
        
        output.push_str("\nConstants:\n");
        for (i, constant) in self.constants.constants.iter().enumerate() {
            output.push_str(&format!("  {}: {:?}\n", i, constant));
        }
        
        output.push_str("\nStrings:\n");
        for (i, string) in self.strings.strings.iter().enumerate() {
            output.push_str(&format!("  {}: {:?}\n", i, string));
        }
        
        output.push_str("\nInstructions:\n");
        for (i, instruction) in self.instructions.iter().enumerate() {
            output.push_str(&format!("  {:04} {}\n", i, Self::disassemble_instruction(instruction)));
        }
        
        output
    }

    fn disassemble_instruction(instruction: &Instruction) -> String {
        match instruction {
            Instruction::LoadConst(idx) => format!("LoadConst {}", idx),
            Instruction::LoadGlobal(idx) => format!("LoadGlobal {}", idx),
            Instruction::StoreGlobal(idx) => format!("StoreGlobal {}", idx),
            Instruction::Add => "Add".to_string(),
            Instruction::Sub => "Sub".to_string(),
            Instruction::Mul => "Mul".to_string(),
            Instruction::Div => "Div".to_string(),
            Instruction::Call(argc) => format!("Call {}", argc),
            Instruction::Pop => "Pop".to_string(),
            Instruction::Return => "Return".to_string(),
        }
    }
}

/// Bytecode error
#[derive(Debug, Clone, PartialEq)]
pub enum BytecodeError {
    InvalidConstantIndex { index: usize, pool_size: usize },
    InvalidStringIndex { index: usize, pool_size: usize },
}

impl std::fmt::Display for BytecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BytecodeError::InvalidConstantIndex { index, pool_size } => {
                write!(
                    f,
                    "Invalid constant index {} (pool size: {})",
                    index, pool_size
                )
            }
            BytecodeError::InvalidStringIndex { index, pool_size } => {
                write!(
                    f,
                    "Invalid string index {} (pool size: {})",
                    index, pool_size
                )
            }
        }
    }
}

impl std::error::Error for BytecodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
        assert!(true);
    }

    #[test]
    fn test_chunk_creation() {
        let chunk = Chunk::new();
        assert_eq!(chunk.instructions.len(), 0);
        assert!(chunk.constants.is_empty());
        assert!(chunk.strings.is_empty());
    }

    #[test]
    fn test_add_instruction() {
        let mut chunk = Chunk::new();
        chunk.add_instruction(Instruction::LoadConst(0));
        chunk.add_instruction(Instruction::Return);
        
        assert_eq!(chunk.instructions.len(), 2);
        assert_eq!(chunk.instructions[0], Instruction::LoadConst(0));
        assert_eq!(chunk.instructions[1], Instruction::Return);
    }

    #[test]
    fn test_constant_pool() {
        let mut pool = ConstantPool::new();
        
        let idx1 = pool.add(Value::Int(42));
        let idx2 = pool.add(Value::Int(100));
        
        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(pool.len(), 2);
        
        assert_eq!(pool.get(0), Some(&Value::Int(42)));
        assert_eq!(pool.get(1), Some(&Value::Int(100)));
        assert_eq!(pool.get(2), None);
    }

    #[test]
    fn test_string_pool_deduplication() {
        let mut pool = StringPool::new();
        
        let idx1 = pool.add("hello".to_string());
        let idx2 = pool.add("world".to_string());
        let idx3 = pool.add("hello".to_string());
        
        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(idx3, 0);
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn test_string_pool_get() {
        let mut pool = StringPool::new();
        pool.add("test".to_string());
        
        assert_eq!(pool.get(0), Some("test"));
        assert_eq!(pool.get(1), None);
    }

    #[test]
    fn test_invalid_constant_index() {
        let pool = ConstantPool::new();
        assert_eq!(pool.get(100), None);
    }

    #[test]
    fn test_bytecode_error_display() {
        let err = BytecodeError::InvalidConstantIndex {
            index: 5,
            pool_size: 3,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("5"));
        assert!(msg.contains("3"));
    }

    #[test]
    fn test_disassemble_empty_chunk() {
        let chunk = Chunk::new();
        let output = chunk.disassemble("test");
        
        assert!(output.contains("=== test ==="));
        assert!(output.contains("Constants:"));
        assert!(output.contains("Strings:"));
        assert!(output.contains("Instructions:"));
    }

    #[test]
    fn test_disassemble_chunk_with_instructions() {
        let mut chunk = Chunk::new();
        let const_idx = chunk.add_constant(Value::Int(42));
        chunk.add_instruction(Instruction::LoadConst(const_idx));
        chunk.add_instruction(Instruction::Return);
        
        let output = chunk.disassemble("test chunk");
        
        assert!(output.contains("LoadConst"));
        assert!(output.contains("Return"));
        assert!(output.contains("Int(42)"));
    }
}
