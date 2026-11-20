//! Calls that a compiler front end and Atlas's qualified resolver read
//! differently. The committed `index.scip` was generated from these bytes.
pub struct Slot(u16);

impl From<Slot> for usize {
    fn from(slot: Slot) -> Self {
        usize::from(slot.0)
    }
}

pub trait Stage {
    fn run(&self) -> u8;
}
pub struct Parse;
impl Stage for Parse {
    fn run(&self) -> u8 {
        base()
    }
}

pub fn base() -> u8 {
    0
}
pub fn label() -> u8 {
    1
}
pub fn direct() -> u8 {
    base()
}
pub fn method_on_local() -> u8 {
    let parse = Parse;
    parse.run()
}
pub fn generic<S: Stage>(stage: &S) -> u8 {
    stage.run()
}
pub fn external() -> usize {
    let text = String::from("ab");
    text.len()
}
pub fn local_value() -> u8 {
    let call = base;
    call()
}
pub fn shadowed_by_item() -> u8 {
    fn label() -> u8 {
        2
    }
    label()
}
pub fn shadowed_by_import() -> u8 {
    use crate::base as label;
    label()
}
pub fn conversion(slot: Slot) -> usize {
    usize::from(slot)
}

macro_rules! define {
    ($name:ident) => {
        pub fn $name() -> u8 {
            7
        }
    };
}
define!(generated);
pub fn calls_generated() -> u8 {
    generated()
}

#[derive(Clone)]
pub struct Copyable;
pub fn derived(value: &Copyable) -> Copyable {
    value.clone()
}
