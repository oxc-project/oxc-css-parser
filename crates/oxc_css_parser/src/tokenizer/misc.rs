use super::token::Ident;
use crate::util;
use std::borrow::Cow;

impl<'s> Ident<'s> {
    #[inline]
    pub fn name(&self) -> Cow<'s, str> {
        if self.escaped { util::handle_escape(self.raw) } else { Cow::from(self.raw) }
    }
}
