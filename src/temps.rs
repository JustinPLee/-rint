use core::fmt;

// Temp generator
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Temp(pub i32);

impl Temp {
    pub fn new(id: i32) -> Self {
        Temp(id)
    }
    pub fn name(self) -> String {
        format!("t{}", self.0)
    }
    pub fn is_special(self) -> bool {
        self.0 < 0
    }
}

#[derive(Debug, Default)]
pub struct TempGen {
    next: i32,
}

impl TempGen {
    pub fn new() -> Self {
        Self { next: 0 }
    }
    pub fn fresh(&mut self) -> Temp {
        let temp = Temp::new(self.next);
        self.next += 1;
        temp
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Label(pub usize);
impl fmt::Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "L{}", self.0)
    }
}

pub struct LabelGen(usize);

impl LabelGen {
    pub fn new() -> Self {
        Self(0)
    }
    pub fn fresh(&mut self) -> Label {
        let label = Label(self.0);
        self.0 += 1;
        label
    }
}
