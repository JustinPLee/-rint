use std::fmt;

// Temp and label generators
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Temp(pub i32);

#[derive(Debug, Default)]
pub struct TempGen(i32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Label(pub i32);

#[derive(Debug, Default)]
pub struct LabelGen(i32);

pub const LABEL_ABORT_NULL_DEREF: Label = Label(-1);

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

impl TempGen {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fresh(&mut self) -> Temp {
        let temp = Temp::new(self.0);
        self.0 += 1;
        temp
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self == &LABEL_ABORT_NULL_DEREF {
            write!(f, ".L_null_deref_abort")
        } else {
            write!(f, ".L{}", self.0)
        }
    }
}

impl LabelGen {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fresh(&mut self) -> Label {
        let label = Label(self.0);
        self.0 += 1;
        label
    }
}

// Pretty printers
pub trait Pretty {
    fn pretty(&self, indent: usize) -> String;
}
#[macro_export]
macro_rules! impl_display_from_pretty {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl std::fmt::Display for $ty {
                fn fmt(
                    &self,
                    f: &mut std::fmt::Formatter<'_>,
                ) -> std::fmt::Result {
                    f.write_str(
                        &<$ty as $crate::utils::Pretty>::pretty(self, 0),
                    )
                }
            }
        )+
    };
}

pub fn write_indent<F: fmt::Write>(f: &mut F, indent: usize) -> fmt::Result {
    write!(f, "{}", "  ".repeat(indent))
}
