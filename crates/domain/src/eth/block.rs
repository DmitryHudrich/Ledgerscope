#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockRef {
    #[default]
    Latest,
    Number(u64),
}
