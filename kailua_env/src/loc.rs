//! Location types and a location-bundled container.

use std::ops;
use std::cmp;
use std::fmt;
use std::borrow::Borrow;

/// An identifier for the code *unit*, unique in the originating `Source`.
///
/// There are two special units, collectively known as "source-independent" units
/// (as they never require `Source` for the resolution):
///
/// * `Unit::dummy()` denotes the lack of source informations.
/// * `Unit::builtin()` is used for all built-in definitions, which are not exposed.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Unit {
    unit: u32,
}

// internal use only, not exposed outside
pub fn unit_from_u32(unit: u32) -> Unit {
    Unit { unit: unit }
}

const BUILTIN_UNIT: u32 = 0xffffffff;

impl Unit {
    pub fn dummy() -> Unit {
        Unit { unit: 0 }
    }

    pub fn builtin() -> Unit {
        Unit { unit: BUILTIN_UNIT }
    }

    pub fn is_dummy(&self) -> bool {
        self.unit == 0
    }

    pub fn is_source_dependent(&self) -> bool {
        self.unit > 0 && self.unit < BUILTIN_UNIT
    }

    pub fn to_usize(&self) -> usize {
        self.unit as usize
    }
}

/// In the debugging output the unit is denoted `@_` or <code>@<i>unit</i></code>.
/// It is only displayed when the alternate flag is enabled.
impl fmt::Debug for Unit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            if self.unit == 0 {
                write!(f, "@_")
            } else if self.unit == BUILTIN_UNIT {
                write!(f, "@<builtin>")
            } else {
                write!(f, "@{}", self.unit)
            }
        } else {
            Ok(())
        }
    }
}

/// A *position* in the originating `Source`.
///
/// The position is composed of the `Unit` and an offset to the corresponding source.
/// An offset ranges from 0 to the length of that source (inclusive).
/// The actual meaning of the offset is resolved by `Source`;
/// it can be either a byte offset or a two-byte word offset (for the FFI compatibility).
/// The "source-independent" units have no corresponding source and the offset is always zero.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos {
    unit: u32,
    pos: u32,
}

// internal use only, not exposed outside
pub fn pos_from_u32(unit: Unit, pos: u32) -> Pos {
    Pos { unit: unit.unit, pos: pos }
}

impl Pos {
    pub fn dummy() -> Pos {
        Pos { unit: 0, pos: 0 }
    }

    pub fn builtin() -> Pos {
        Pos { unit: BUILTIN_UNIT, pos: 0 }
    }

    pub fn is_dummy(&self) -> bool {
        self.unit().is_dummy()
    }

    pub fn is_source_dependent(&self) -> bool {
        self.unit().is_source_dependent()
    }

    pub fn unit(&self) -> Unit {
        Unit { unit: self.unit }
    }

    pub fn to_usize(&self) -> usize {
        self.pos as usize
    }
}

/// In the debugging output the position is denoted `@_` or <code>@<i>unit</i>/<i>off</i></code>.
/// It is only displayed when the alternate flag is enabled.
impl fmt::Debug for Pos {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            if self.unit == 0 {
                write!(f, "@_")
            } else if self.unit == BUILTIN_UNIT {
                write!(f, "@<builtin>")
            } else {
                write!(f, "@{}/{}", self.unit, self.pos)
            }
        } else {
            Ok(())
        }
    }
}

/// A *span* of the range in the originating `Source`.
///
/// The span is (conceptually) composed of two `Pos` with the same `Unit`.
/// The first `Pos` (inclusive) should be ahead of the second `Pos` (exclusive);
/// they can be equal to each other, in which case the span has the same meaning to `Pos`.
/// The "source-independent" units have no corresponding source and the offsets are always zero.
//
// span (0, 0, 0) is dummy and indicates the absence of appropriate span infos.
// span (0, y, z) for non-zero y and z is reserved.
// span (x, y, y) for non-zero x and y indicates a point and can be lifted from Pos.
// span (x, y, z) for non-zero x, y and z (y < z) is an ordinary span, with z exclusive.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    unit: u32,
    begin: u32,
    end: u32,
}

// internal use only, not exposed outside
pub fn span_from_u32(unit: Unit, begin: u32, end: u32) -> Span {
    Span { unit: unit.unit, begin: begin, end: end }
}

impl Span {
    pub fn new(begin: Pos, end: Pos) -> Span {
        if begin.is_dummy() || end.is_dummy() {
            Span::dummy()
        } else {
            assert!(begin.unit == end.unit, "Span::new with positions from different units");
            if begin.pos <= end.pos {
                Span { unit: begin.unit, begin: begin.pos, end: end.pos }
            } else {
                // this is possible when the range actually describes an empty span.
                // in the ordinary case we take the beginning of the first token and
                // the end of the last token for the span:
                //
                // function f()    FIRST_TOKEN ... LAST_TOKEN    end
                //                 ^ begin               end ^
                //
                // but if the span is empty, the order is swapped:
                //
                // function f()    end
                //         end ^   ^ begin
                //
                // the most reasonable choice here would be using (end..begin)
                // as an indication of the empty span.
                Span { unit: begin.unit, begin: end.pos, end: begin.pos }
            }
        }
    }

    pub fn dummy() -> Span {
        Span { unit: 0, begin: 0, end: 0 }
    }

    pub fn builtin() -> Span {
        Span { unit: BUILTIN_UNIT, begin: 0, end: 0 }
    }

    pub fn is_dummy(&self) -> bool {
        self.unit().is_dummy()
    }

    pub fn is_source_dependent(&self) -> bool {
        self.unit().is_source_dependent()
    }

    pub fn to_pos(&self) -> Pos {
        if self.begin == self.end {
            Pos { unit: self.unit, pos: self.begin }
        } else {
            Pos::dummy()
        }
    }

    pub fn unit(&self) -> Unit {
        Unit { unit: self.unit }
    }

    pub fn begin(&self) -> Pos {
        Pos { unit: self.unit, pos: self.begin }
    }

    pub fn end(&self) -> Pos {
        Pos { unit: self.unit, pos: self.end }
    }

    pub fn len(&self) -> usize {
        if self.is_source_dependent() {
            (self.end - self.begin) as usize
        } else {
            0
        }
    }

    pub fn contains(&self, pos: Pos) -> bool {
        self.unit > 0 && self.unit == pos.unit && self.begin <= pos.pos && pos.pos < self.end
    }

    pub fn contains_or_end(&self, pos: Pos) -> bool {
        self.unit > 0 && self.unit == pos.unit && self.begin <= pos.pos && pos.pos <= self.end
    }
}

impl ops::BitAnd for Span {
    type Output = Span;
    fn bitand(self, other: Span) -> Span {
        if self.is_dummy() || other.is_dummy() { return Span::dummy(); }
        if self.unit == other.unit {
            let begin = cmp::max(self.begin, other.begin);
            let end = cmp::min(self.end, other.end);
            if begin > end { return Span::dummy(); }
            Span { unit: self.unit, begin: begin, end: end }
        } else {
            Span::dummy()
        }
    }
}

impl ops::BitAndAssign for Span {
    fn bitand_assign(&mut self, other: Span) { *self = *self & other; }
}

impl ops::BitOr for Span {
    type Output = Span;
    fn bitor(self, other: Span) -> Span {
        if self.is_dummy() { return other; }
        if other.is_dummy() { return self; }
        if self.unit == other.unit {
            Span {
                unit: self.unit,
                begin: cmp::min(self.begin, other.begin),
                end: cmp::max(self.end, other.end),
            }
        } else {
            Span::dummy()
        }
    }
}

impl ops::BitOrAssign for Span {
    fn bitor_assign(&mut self, other: Span) { *self = *self | other; }
}

/// The span can be used as an iterator and yields all positions in the span.
impl Iterator for Span {
    type Item = Pos;

    fn next(&mut self) -> Option<Pos> {
        if self.is_source_dependent() && self.begin < self.end {
            let pos = Pos { unit: self.unit, pos: self.begin };
            self.begin += 1;
            Some(pos)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }

    fn count(self) -> usize {
        self.len()
    }

    fn last(self) -> Option<Pos> {
        if self.is_source_dependent() && self.begin < self.end {
            Some(Pos { unit: self.unit, pos: self.end - 1 })
        } else {
            None
        }
    }

    fn nth(&mut self, n: usize) -> Option<Pos> {
        if self.is_source_dependent() && n < (self.end - self.begin) as usize {
            let pos = Pos { unit: self.unit, pos: self.begin + n as u32 };
            self.begin += n as u32;
            Some(pos)
        } else {
            None
        }
    }
}

/// The span can be used as an iterator and yields all positions in the span.
impl DoubleEndedIterator for Span {
    fn next_back(&mut self) -> Option<Pos> {
        if self.is_source_dependent() && self.begin < self.end {
            let pos = Pos { unit: self.unit, pos: self.end - 1 };
            self.end -= 1;
            Some(pos)
        } else {
            None
        }
    }
}

/// In the debugging output the span is denoted `@_` or
/// <code>@<i>unit</i>/<i>start</i>[-<i>end</i>]</code>.
/// It is only displayed when the alternate flag is enabled.
impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            if self.unit == 0 {
                write!(f, "@_")
            } else if self.unit == BUILTIN_UNIT {
                write!(f, "@<builtin>")
            } else if self.begin == self.end {
                write!(f, "@{}/{}", self.unit, self.begin)
            } else {
                write!(f, "@{}/{}-{}", self.unit, self.begin, self.end)
            }
        } else {
            Ok(())
        }
    }
}

/// A value with optional `Span`.
///
/// Can be constructed with `.with_loc(span)` or `.without_loc()` from the `WithLoc` trait.
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub span: Span,
    pub base: T,
}

impl<T> Spanned<T> {
    pub fn as_ref(&self) -> Spanned<&T> {
        Spanned { span: self.span, base: &self.base }
    }

    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Spanned<U> {
        Spanned { span: self.span, base: f(self.base) }
    }
}

impl From<Pos> for Span {
    fn from(pos: Pos) -> Span {
        Span { unit: pos.unit, begin: pos.pos, end: pos.pos }
    }
}

impl From<ops::Range<Pos>> for Span {
    fn from(range: ops::Range<Pos>) -> Span {
        Span::new(range.start, range.end)
    }
}

impl<T> From<Spanned<T>> for Span {
    fn from(spanned: Spanned<T>) -> Span { spanned.span }
}

impl<'a, T> From<&'a Spanned<T>> for Span {
    fn from(spanned: &'a Spanned<T>) -> Span { spanned.span }
}

impl<'a, T> From<&'a mut Spanned<T>> for Span {
    fn from(spanned: &'a mut Spanned<T>) -> Span { spanned.span }
}

impl<T> ops::Deref for Spanned<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.base }
}

impl<T> ops::DerefMut for Spanned<T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.base }
}

impl<T> Borrow<T> for Spanned<T> {
    fn borrow(&self) -> &T { &self.base }
}

/// The span is ignored in the display.
impl<T: fmt::Display> fmt::Display for Spanned<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.base, f)
    }
}

/// The span is only printed (after the value) when the alternate flag is enabled.
impl<T: fmt::Debug> fmt::Debug for Spanned<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.base, f)?;
        fmt::Debug::fmt(&self.span, f)?;
        Ok(())
    }
}

/// A helper trait for constructing `Spanned<T>` value.
pub trait WithLoc: Sized {
    fn with_loc<Loc: Into<Span>>(self, loc: Loc) -> Spanned<Self> {
        Spanned { span: loc.into(), base: self }
    }

    fn without_loc(self) -> Spanned<Self> {
        Spanned { span: Span::dummy(), base: self }
    }
}

impl<T> WithLoc for T {}

