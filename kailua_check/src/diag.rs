use std::fmt;
use std::cell::RefCell;

use kailua_env::{Span, Spanned, WithLoc};
use kailua_diag::report::ReportMore;
use kailua_diag::message::{Locale, Localize, Localized};
use message as m;
use ty::{TypeContext, Display, Key};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ordinal(pub usize);

impl Localize for Ordinal {
    fn fmt_localized(&self, f: &mut fmt::Formatter, locale: Locale) -> fmt::Result {
        match &locale[..] {
            "ko" => {
                let w = match self.0 {
                    0 => "첫번째",
                    1 => "두번째",
                    2 => "세번째",
                    3 => "네번째",
                    4 => "다섯번째",
                    5 => "여섯번째",
                    6 => "일곱번째",
                    7 => "여덟번째",
                    8 => "아홉번째",
                    i => return write!(f, "{}번째", i + 1),
                };
                write!(f, "{}", w)
            },

            _ => {
                let (wc, wl) = match self.0 {
                    0 => ("First",   "first"),
                    1 => ("Second",  "second"),
                    2 => ("Third",   "third"),
                    3 => ("Fourth",  "fourth"),
                    4 => ("Fifth",   "fifth"),
                    5 => ("Sixth",   "sixth"),
                    6 => ("Seventh", "seventh"),
                    7 => ("Eighth",  "eighth"),
                    8 => ("Ninth",   "ninth"),
                    i if i % 10 == 0 && i % 100 != 10 => return write!(f, "{}-st", i + 1),
                    i if i % 10 == 1 && i % 100 != 10 => return write!(f, "{}-nd", i + 1),
                    i if i % 10 == 2 && i % 100 != 10 => return write!(f, "{}-rd", i + 1),
                    i => return write!(f, "{}-th", i + 1),
                };
                if f.sign_plus() {
                    write!(f, "{}", wc)
                } else {
                    write!(f, "{}", wl)
                }
            },
        }
    }
}

struct QuotedList<'a, I: Iterator<Item=&'a Localize>> {
    iter: RefCell<Option<I>>,
    locale: Locale,
}

impl<'a, I: Iterator<Item=&'a Localize>> QuotedList<'a, I> {
    fn new(iter: I, locale: Locale) -> QuotedList<'a, I> {
        QuotedList { iter: RefCell::new(Some(iter)), locale: locale }
    }
}

impl<'a, I: Iterator<Item=&'a Localize>> fmt::Display for QuotedList<'a, I> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut iter = self.iter.borrow_mut().take().expect(
            "QuotedList can be formatted only once"
        );

        let mut cur = if let Some(cur) = iter.next() {
            cur
        } else {
            return Ok(());
        };

        write!(f, "`")?;

        // delayed iteration to handle the last "and"
        let mut first = true;
        while let Some(next) = iter.next() {
            if first {
                first = false;
            } else {
                write!(f, "`, `")?;
            }

            // keep the formatter flags down to elements
            cur.fmt_localized(f, self.locale)?;
            cur = next;
        }

        if !first {
            match &self.locale[..] {
                "ko" => write!(f, "` 및 `")?,
                _ => write!(f, "` and `")?,
            }
        }
        cur.fmt_localized(f, self.locale)?;
        write!(f, "`")
    }
}

// type-level report is used to collect hierarchical information about failures.
// this is distinct from `kailua_diag::Result` which is non-hierarchical by nature.
#[derive(Clone, Debug)]
pub struct TypeReport {
    locale: Locale,
    messages: Vec<ReportItem>, // in the reverse chronological order
}

// a single report item can be composed of multiple errors at each layer of type operation.
// since showing all of them is not desirable, we use "origins" to distinguish each layer
// and to determine whether to flatten the item.
//
// for this reason origins are hierarchically ordered and the origin at higher classes can
// only be preceded by the origin at lower classes. if the origin "inversion" does occur,
// that would mean that an _irrelevant_ report item has been placed and should not be flattened.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    RVar = 0x00, TVar = 0x01,
    Tables = 0x10, Numbers = 0x11, Strings = 0x12, Functions = 0x13,
    Union = 0x20,
    TUnion = 0x30, // binary operation between T and Union
    T = 0x40,
    TTy = 0x50, // binary operation between T and Ty
    Ty = 0x60,
    Slot = 0x70,
}

impl Origin {
    fn origin_class(&self) -> u8 {
        *self as u8 >> 4
    }

    pub fn can_overwrite(&self, prev: Origin) -> bool {
        let self_class = self.origin_class();
        let prev_class = prev.origin_class();
        self_class > prev_class
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum BinaryReportKind {
    NotSubtype,
    NotEqual,
    CannotUnion(bool /*explicit*/),
}

#[derive(Clone, Debug)]
enum ReportItem {
    Binary(BinaryReportKind, Origin, Spanned<String>, Spanned<String>, Option<usize>),
    LessArity(Span, Spanned<String>, usize),
    MoreArity(Spanned<String>, Span, usize),
    CannotUnionSingle(Spanned<String>),
    CannotAssign(Origin, Spanned<String>, Spanned<String>),
    CannotUpdate(Origin, Spanned<String>),
    CannotFilter(Origin, Spanned<String>),
    InextensibleRec(Span),
    RecursiveRec(Span),
    RecDuplicateKey(Span, Spanned<Key>),
    RecCannotHaveKey(Span, Spanned<Key>),
    RecShouldHaveKeys(Span, Spanned<Vec<Key>>),
    RecExtendedWithNonNil(Span, Spanned<Key>, Spanned<String>),
}

impl TypeReport {
    pub fn new(locale: Locale) -> TypeReport {
        TypeReport { locale: locale, messages: Vec::new() }
    }

    fn binary<T: Display, U: Display>(mut self, kind: BinaryReportKind, org: Origin,
                                      lhs: T, rhs: U, ctx: &TypeContext) -> TypeReport {
        trace!("TypeReport::binary({:?}, {:?}, {:?}, {:?})", kind, org, lhs, rhs);

        let locale = self.locale;
        let lhs = Localized::new(&lhs.display(ctx), locale).to_string();
        let rhs = Localized::new(&rhs.display(ctx), locale).to_string();

        let item = match self.messages.pop() {
            Some(ReportItem::Binary(kind_, org_, Spanned { span: lhsspan, .. },
                                                 Spanned { span: rhsspan, .. }, idx))
                if kind == kind_ && org.can_overwrite(org_) &&
                   lhsspan.is_dummy() && rhsspan.is_dummy()
            => {
                ReportItem::Binary(kind, org, lhs.with_loc(lhsspan), rhs.with_loc(rhsspan), idx)
            },

            // RVar origin is mostly useless except when it appears alone
            Some(ReportItem::Binary(_, Origin::RVar, Spanned { span: lhsspan, .. },
                                                     Spanned { span: rhsspan, .. }, idx)) => {
                ReportItem::Binary(kind, org, lhs.with_loc(lhsspan), rhs.with_loc(rhsspan), idx)
            },

            last => {
                if let Some(last) = last {
                    self.messages.push(last); // push back an irrelevant item
                }
                ReportItem::Binary(kind, org, lhs.without_loc(), rhs.without_loc(), None)
            },
        };

        self.messages.push(item);
        self
    }

    fn binary_attach_span(mut self, kind: BinaryReportKind,
                          lhsspan: Span, rhsspan: Span) -> TypeReport {
        trace!("TypeReport::binary_attach_span({:?}, {:#?}, {:#?})", kind, lhsspan, rhsspan);

        match self.messages.last_mut() {
            Some(&mut ReportItem::Binary(kind_, _org, ref mut lhs, ref mut rhs, _idx))
                if kind == kind_ && lhs.span.is_dummy() && rhs.span.is_dummy()
            => {
                lhs.span = lhsspan;
                rhs.span = rhsspan;
            }
            last => {
                panic!("TypeReport::binary_attach_span({:?}, {:#?}, {:#?}) \
                        called with the last report item {:#?}", kind, lhsspan, rhsspan, last);
            }
        }
        self
    }

    fn binary_attach_index(mut self, kind: BinaryReportKind,
                           org: Origin, index: usize) -> TypeReport {
        trace!("TypeReport::binary_attach_index({:?}, {:?}, {})", kind, org, index);

        match self.messages.last_mut() {
            Some(&mut ReportItem::Binary(ref kind_, ref org_, _, _, ref mut idx))
                if kind == *kind_ && org == *org_ && idx.is_none()
            => {
                *idx = Some(index);
            }
            last => {
                panic!("TypeReport::binary_attach_index({:?}, {:?}, {}) \
                        called with the last report item {:#?}", kind, org, index, last);
            }
        }
        self
    }

    pub fn not_sub<T: Display, U: Display>(self, org: Origin, lhs: T, rhs: U,
                                           ctx: &TypeContext) -> TypeReport {
        self.binary(BinaryReportKind::NotSubtype, org, lhs, rhs, ctx)
    }

    pub fn not_sub_attach_span(self, lhsspan: Span, rhsspan: Span) -> TypeReport {
        self.binary_attach_span(BinaryReportKind::NotSubtype, lhsspan, rhsspan)
    }

    pub fn not_sub_attach_index(self, org: Origin, index: usize) -> TypeReport {
        self.binary_attach_index(BinaryReportKind::NotSubtype, org, index)
    }

    pub fn not_eq<T: Display, U: Display>(self, org: Origin, lhs: T, rhs: U,
                                          ctx: &TypeContext) -> TypeReport {
        self.binary(BinaryReportKind::NotEqual, org, lhs, rhs, ctx)
    }

    pub fn not_eq_attach_span(self, lhsspan: Span, rhsspan: Span) -> TypeReport {
        self.binary_attach_span(BinaryReportKind::NotEqual, lhsspan, rhsspan)
    }

    pub fn not_eq_attach_index(self, org: Origin, index: usize) -> TypeReport {
        self.binary_attach_index(BinaryReportKind::NotEqual, org, index)
    }

    pub fn cannot_union<T: Display, U: Display>(self, org: Origin, lhs: T, rhs: U, explicit: bool,
                                                ctx: &TypeContext) -> TypeReport {
        self.binary(BinaryReportKind::CannotUnion(explicit), org, lhs, rhs, ctx)
    }

    pub fn cannot_union_attach_span(self, lhsspan: Span, rhsspan: Span,
                                    explicit: bool) -> TypeReport {
        self.binary_attach_span(BinaryReportKind::CannotUnion(explicit), lhsspan, rhsspan)
    }

    pub fn cannot_union_attach_index(self, org: Origin, index: usize,
                                     explicit: bool) -> TypeReport {
        self.binary_attach_index(BinaryReportKind::CannotUnion(explicit), org, index)
    }

    pub fn cannot_union_single<T: Display>(mut self, t: T, ctx: &TypeContext) -> TypeReport {
        let locale = self.locale;
        let t = Localized::new(&t.display(ctx), locale).to_string().without_loc(); // TODO span
        self.messages.push(ReportItem::CannotUnionSingle(t));
        self
    }

    pub fn less_arity<T: Display>(mut self, lhs: Span, rhs: Spanned<&T>, index: usize,
                                  ctx: &TypeContext) -> TypeReport {
        let locale = self.locale;
        let rhs = rhs.map(|t| Localized::new(&t.display(ctx), locale).to_string());
        self.messages.push(ReportItem::LessArity(lhs, rhs, index));
        self
    }

    pub fn more_arity<T: Display>(mut self, lhs: Spanned<&T>, rhs: Span, index: usize,
                                  ctx: &TypeContext) -> TypeReport {
        let locale = self.locale;
        let lhs = lhs.map(|t| Localized::new(&t.display(ctx), locale).to_string());
        self.messages.push(ReportItem::MoreArity(lhs, rhs, index));
        self
    }

    pub fn cannot_assign<T: Display, U: Display>(mut self, org: Origin, t: T, u: U,
                                                 ctx: &TypeContext) -> TypeReport {
        let locale = self.locale;
        let lhs = Localized::new(&t.display(ctx), locale).to_string().without_loc(); // TODO span
        let rhs = Localized::new(&u.display(ctx), locale).to_string().without_loc(); // TODO span
        self.messages.push(ReportItem::CannotAssign(org, lhs, rhs));
        self
    }

    pub fn cannot_assign_in_place<T: Display>(mut self, org: Origin, t: T,
                                              ctx: &TypeContext) -> TypeReport {
        let locale = self.locale;
        let t = Localized::new(&t.display(ctx), locale).to_string().without_loc(); // TODO span
        self.messages.push(ReportItem::CannotUpdate(org, t));
        self
    }

    pub fn cannot_filter_by_flags<T: Display>(mut self, org: Origin, t: T,
                                              ctx: &TypeContext) -> TypeReport {
        let locale = self.locale;
        let t = Localized::new(&t.display(ctx), locale).to_string().without_loc(); // TODO span
        self.messages.push(ReportItem::CannotFilter(org, t));
        self
    }

    pub fn inextensible_record(mut self) -> TypeReport {
        self.messages.push(ReportItem::InextensibleRec(Span::dummy())); // TODO span
        self
    }

    pub fn recursive_record(mut self) -> TypeReport {
        self.messages.push(ReportItem::RecursiveRec(Span::dummy())); // TODO span
        self
    }

    pub fn record_duplicate_key(mut self, k: &Key) -> TypeReport {
        let k = k.clone().without_loc(); // TODO span
        self.messages.push(ReportItem::RecDuplicateKey(Span::dummy(), k)); // TODO span
        self
    }

    pub fn record_cannot_have_key(mut self, k: &Key) -> TypeReport {
        let k = k.clone().without_loc(); // TODO span
        self.messages.push(ReportItem::RecCannotHaveKey(Span::dummy(), k)); // TODO span
        self
    }

    pub fn record_should_have_keys<'a, Keys>(mut self, keys: Keys) -> TypeReport
        where Keys: Iterator<Item=&'a Key>
    {
        let keys = keys.map(|k| k.clone()).collect::<Vec<_>>().without_loc(); // TODO span
        self.messages.push(ReportItem::RecShouldHaveKeys(Span::dummy(), keys)); // TODO span
        self
    }

    pub fn record_extended_with_non_nil<T: Display>(mut self, k: &Key, v: T,
                                                    ctx: &TypeContext) -> TypeReport {
        let locale = self.locale;
        let k = k.clone().without_loc(); // TODO span
        let v = Localized::new(&v.display(ctx), locale).to_string().without_loc(); // TODO span
        self.messages.push(ReportItem::RecExtendedWithNonNil(Span::dummy(), k, v)); // TODO span
        self
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TypeReportHint {
    None,
    FuncArgs,
    MethodArgs,
    Returns,
}

pub trait TypeReportMore {
    fn report_types(self, r: TypeReport, hint: TypeReportHint) -> Self;
}

impl<'a, T> TypeReportMore for ReportMore<'a, T> {
    fn report_types(mut self, r: TypeReport, mut hint: TypeReportHint) -> ReportMore<'a, T> {
        trace!("collected type reports: {:#?}", r);

        fn report_binary<
            'a, 'b, T,
            M: 'b + Localize, Msg: FnOnce(&'b str, &'b str) -> M,
            MS: 'b + Localize, MsgSelf: FnOnce(&'b str, &'b str) -> MS,
            MFA: 'b + Localize, MsgFuncArgs: FnOnce(&'b str, &'b str, Ordinal) -> MFA,
            MMA: 'b + Localize, MsgMethodArgs: FnOnce(&'b str, &'b str, Ordinal) -> MMA,
            MR: 'b + Localize, MsgReturns: FnOnce(&'b str, &'b str, Ordinal) -> MR,
        >(
            more: ReportMore<'a, T>, lhs: &'b Spanned<String>, rhs: &'b Spanned<String>,
            idx: Option<usize>, hint: &mut TypeReportHint,
            make_msg: Msg, make_msg_in_self: MsgSelf, make_msg_in_func_args: MsgFuncArgs,
            make_msg_in_method_args: MsgMethodArgs, make_msg_in_returns: MsgReturns,
        ) -> ReportMore<'a, T> {
            match idx {
                Some(idx) if *hint == TypeReportHint::FuncArgs => {
                    *hint = TypeReportHint::None;
                    if lhs.span.is_dummy() && !rhs.span.is_dummy() {
                        more.cause(rhs, make_msg_in_func_args(lhs, rhs, Ordinal(idx)))
                    } else {
                        more.cause(lhs, make_msg_in_func_args(lhs, rhs, Ordinal(idx)))
                            .note_if(rhs, m::OtherTypeOrigin {})
                    }
                },

                Some(0) if *hint == TypeReportHint::MethodArgs => {
                    *hint = TypeReportHint::None;
                    if lhs.span.is_dummy() && !rhs.span.is_dummy() {
                        more.cause(rhs, make_msg_in_self(lhs, rhs))
                    } else {
                        more.cause(lhs, make_msg_in_self(lhs, rhs))
                            .note_if(rhs, m::OtherTypeOrigin {})
                    }
                },

                Some(idx) if *hint == TypeReportHint::MethodArgs => {
                    *hint = TypeReportHint::None;
                    if lhs.span.is_dummy() && !rhs.span.is_dummy() {
                        more.cause(rhs, make_msg_in_method_args(lhs, rhs, Ordinal(idx - 1)))
                    } else {
                        more.cause(lhs, make_msg_in_method_args(lhs, rhs, Ordinal(idx - 1)))
                            .note_if(rhs, m::OtherTypeOrigin {})
                    }
                },

                Some(idx) if *hint == TypeReportHint::Returns => {
                    *hint = TypeReportHint::None;
                    if lhs.span.is_dummy() && !rhs.span.is_dummy() {
                        more.cause(rhs, make_msg_in_returns(lhs, rhs, Ordinal(idx)))
                    } else {
                        more.cause(lhs, make_msg_in_returns(lhs, rhs, Ordinal(idx)))
                            .note_if(rhs, m::OtherTypeOrigin {})
                    }
                },

                _ => {
                    if lhs.span.is_dummy() && !rhs.span.is_dummy() {
                        more.cause(rhs, make_msg(lhs, rhs))
                    } else {
                        more.cause(lhs, make_msg(lhs, rhs))
                            .note_if(rhs, m::OtherTypeOrigin {})
                    }
                },
            }
        }

        for item in r.messages.into_iter().rev() {
            match item {
                ReportItem::Binary(BinaryReportKind::NotSubtype, _org, ref sub, ref sup, idx) => {
                    self = report_binary(
                        self, sub, sup, idx, &mut hint,
                        |sub, sup| m::NotSubtype { sub: sub, sup: sup },
                        |sub, sup| m::NotSubtypeInSelf { sub: sub, sup: sup },
                        |sub, sup, i| m::NotSubtypeInFuncArgs { sub: sub, sup: sup, index: i },
                        |sub, sup, i| m::NotSubtypeInMethodArgs { sub: sub, sup: sup, index: i },
                        |sub, sup, i| m::NotSubtypeInReturns { sub: sub, sup: sup, index: i },
                    )
                }

                ReportItem::Binary(BinaryReportKind::NotEqual, _org, ref lhs, ref rhs, idx) => {
                    self = report_binary(
                        self, lhs, rhs, idx, &mut hint,
                        |lhs, rhs| m::NotEqual { lhs: lhs, rhs: rhs },
                        |lhs, rhs| m::NotEqualInSelf { lhs: lhs, rhs: rhs },
                        |lhs, rhs, i| m::NotEqualInFuncArgs { lhs: lhs, rhs: rhs, index: i },
                        |lhs, rhs, i| m::NotEqualInMethodArgs { lhs: lhs, rhs: rhs, index: i },
                        |lhs, rhs, i| m::NotEqualInReturns { lhs: lhs, rhs: rhs, index: i },
                    )
                }

                ReportItem::Binary(BinaryReportKind::CannotUnion(_explicit), _org,
                                   ref lhs, ref rhs, idx) => {
                    self = report_binary(
                        self, lhs, rhs, idx, &mut hint,
                        |lhs, rhs| m::InvalidUnionType { lhs: lhs, rhs: rhs },
                        |lhs, rhs| m::InvalidUnionTypeInSelf { lhs: lhs, rhs: rhs },
                        |lhs, rhs, i| m::InvalidUnionTypeInFuncArgs { lhs: lhs, rhs: rhs,
                                                                      index: i },
                        |lhs, rhs, i| m::InvalidUnionTypeInMethodArgs { lhs: lhs, rhs: rhs,
                                                                        index: i },
                        |lhs, rhs, i| m::InvalidUnionTypeInReturns { lhs: lhs, rhs: rhs,
                                                                     index: i },
                    )
                }

                ReportItem::LessArity(lhs, ref rhs, idx) if hint == TypeReportHint::FuncArgs => {
                    hint = TypeReportHint::None;
                    self = self.cause(lhs,
                                      m::LessArityInFuncArgs { other: rhs, index: Ordinal(idx) })
                               .note_if(rhs, m::OtherTypeOrigin {});
                }

                ReportItem::MoreArity(ref lhs, rhs, idx) if hint == TypeReportHint::FuncArgs => {
                    hint = TypeReportHint::None;
                    self = self.cause(rhs, m::MoreArityInFuncArgs { index: idx })
                               .note_if(lhs, m::OtherTypeOrigin {});
                }

                ReportItem::LessArity(lhs, ref rhs, idx) if hint == TypeReportHint::MethodArgs => {
                    hint = TypeReportHint::None;
                    self = self.cause(lhs,
                                      m::LessArityInMethodArgs { other: rhs, index: Ordinal(idx) })
                               .note_if(rhs, m::OtherTypeOrigin {});
                }

                ReportItem::MoreArity(ref lhs, rhs, idx) if hint == TypeReportHint::MethodArgs => {
                    hint = TypeReportHint::None;
                    self = self.cause(rhs, m::MoreArityInMethodArgs { index: idx })
                               .note_if(lhs, m::OtherTypeOrigin {});
                }

                ReportItem::LessArity(lhs, ref rhs, idx) if hint == TypeReportHint::Returns => {
                    hint = TypeReportHint::None;
                    self = self.cause(rhs,
                                      m::LessArityInReturns { other: rhs, index: Ordinal(idx) })
                               .note_if(lhs, m::OtherTypeOrigin {});
                }

                ReportItem::MoreArity(ref lhs, rhs, idx) if hint == TypeReportHint::Returns => {
                    hint = TypeReportHint::None;
                    self = self.cause(rhs, m::MoreArityInReturns { index: idx })
                               .note_if(lhs, m::OtherTypeOrigin {});
                }

                ReportItem::LessArity(lhs, ref rhs, idx) => {
                    self = self.cause(lhs, m::ArityMismatch { other: rhs, index: Ordinal(idx) })
                               .note_if(rhs, m::OtherTypeOrigin {});
                }

                ReportItem::MoreArity(ref lhs, rhs, idx) => {
                    self = self.cause(rhs, m::ArityMismatch { other: lhs, index: Ordinal(idx) })
                               .note_if(lhs, m::OtherTypeOrigin {});
                }

                ReportItem::CannotUnionSingle(ref ty) => {
                    self = self.cause(ty, m::CannotUnionType { ty: ty });
                }

                ReportItem::CannotAssign(_org, ref lhs, ref rhs) => {
                    self = self.cause(lhs, m::CannotAssignInner { lhs: lhs, rhs: rhs })
                               .note_if(rhs, m::OtherTypeOrigin {});
                }

                ReportItem::CannotUpdate(_org, ref tab) => {
                    self = self.cause(tab, m::CannotUpdateInner { tab: tab });
                }

                ReportItem::CannotFilter(_org, ref ty) => {
                    self = self.cause(ty, m::CannotFilterInner { ty: ty });
                }

                ReportItem::InextensibleRec(recspan) => {
                    self = self.cause(recspan, m::InextensibleRec {});
                }

                ReportItem::RecursiveRec(recspan) => {
                    self = self.cause(recspan, m::RecursiveRec {});
                }

                ReportItem::RecDuplicateKey(recspan, ref key) => {
                    // TODO do something with key.span
                    self = self.cause(recspan, m::RecDuplicateKey { key: key });
                }

                ReportItem::RecCannotHaveKey(recspan, ref key) => {
                    // TODO do something with key.span
                    self = self.cause(recspan, m::RecCannotHaveKey { key: key });
                }

                ReportItem::RecShouldHaveKeys(recspan, ref keys) => {
                    // TODO do something with keys.span
                    let keys = QuotedList::new(keys.iter().map(|k| k as &Localize), r.locale);
                    self = self.cause(recspan, m::RecShouldHaveKeys { keys: &keys.to_string() });
                }

                ReportItem::RecExtendedWithNonNil(recspan, ref key, ref value) => {
                    // TODO do something with key.span and value.span
                    self = self.cause(recspan, m::RecExtendedWithNonNil { key: key, slot: value });
                }
            }
        }

        self
    }
}

pub type TypeResult<T> = Result<T, TypeReport>;

