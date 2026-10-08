//! Event grammar shared by both parsers: complete on `&[u8]`, resumable on `Partial<&[u8]>`.

use winnow::{
    ModalResult, Parser,
    binary::{be_f32, be_f64, be_i32, be_i64, be_u8, be_u16},
    error::{AddContext, ErrMode, Needed, ParserError},
    stream::{Stream, StreamIsPartial},
    token::take,
};

use crate::{
    AbxError, Attribute, AttributeValue, CMD_ATTRIBUTE, CMD_CDSECT, CMD_COMMENT, CMD_DOCDECL,
    CMD_END_DOCUMENT, CMD_END_TAG, CMD_ENTITY_REF, CMD_IGNORABLE_WHITESPACE,
    CMD_PROCESSING_INSTRUCTION, CMD_START_DOCUMENT, CMD_START_TAG, CMD_TEXT, Event, INTERNED_NEW,
    InternedStr, TYPE_BOOLEAN_FALSE, TYPE_BOOLEAN_TRUE, TYPE_BYTES_BASE64, TYPE_BYTES_HEX,
    TYPE_DOUBLE, TYPE_FLOAT, TYPE_INT, TYPE_INT_HEX, TYPE_LONG, TYPE_LONG_HEX, TYPE_NULL,
    TYPE_STRING, TYPE_STRING_INTERNED,
};

/// Private so winnow's traits stay out of `AbxError`'s public impls.
#[derive(Debug)]
pub(crate) struct DecodeError(pub(crate) AbxError);

impl<I: Stream> ParserError<I> for DecodeError {
    type Inner = Self;

    fn from_input(_input: &I) -> Self {
        DecodeError(AbxError::UnexpectedEof("primitive"))
    }

    fn into_inner(self) -> Result<Self, Self> {
        Ok(self)
    }
}

impl<I: Stream> AddContext<I, &'static str> for DecodeError {
    fn add_context(self, _input: &I, _start: &I::Checkpoint, ctx: &'static str) -> Self {
        match self.0 {
            AbxError::UnexpectedEof(_) => DecodeError(AbxError::UnexpectedEof(ctx)),
            other => DecodeError(other),
        }
    }
}

pub(crate) type PResult<T> = ModalResult<T, DecodeError>;

pub(crate) trait AbxInput<'i>:
    Stream<Token = u8, Slice = &'i [u8]> + StreamIsPartial
{
}
impl<'i, T: Stream<Token = u8, Slice = &'i [u8]> + StreamIsPartial> AbxInput<'i> for T {}

fn fail<T>(e: AbxError) -> PResult<T> {
    Err(ErrMode::Cut(DecodeError(e)))
}

fn payload<'i, I: AbxInput<'i>>(i: &mut I, what: &'static str) -> PResult<&'i [u8]> {
    let len = be_u16(i)?;
    take(len).context(what).parse_next(i)
}

fn utf<'i, I: AbxInput<'i>>(i: &mut I) -> PResult<String> {
    let bytes = payload(i, "UTF string payload")?;
    match std::str::from_utf8(bytes) {
        Ok(s) => Ok(s.to_owned()),
        Err(_) => fail(AbxError::InvalidUtf8),
    }
}

fn bytes_blob<'i, I: AbxInput<'i>>(i: &mut I) -> PResult<Vec<u8>> {
    payload(i, "bytes payload").map(<[u8]>::to_vec)
}

fn interned<'i, I: AbxInput<'i>>(i: &mut I, pool: &mut Vec<InternedStr>) -> PResult<InternedStr> {
    let idx = be_u16(i)?;
    if idx == INTERNED_NEW {
        let s: InternedStr = utf(i)?.into();
        pool.push(s.clone());
        Ok(s)
    } else {
        match pool.get(idx as usize) {
            Some(s) => Ok(s.clone()),
            None => fail(AbxError::BadInternedIndex(idx)),
        }
    }
}

fn attr_value<'i, I: AbxInput<'i>>(
    i: &mut I,
    pool: &mut Vec<InternedStr>,
    type_nibble: u8,
) -> PResult<AttributeValue> {
    Ok(match type_nibble {
        TYPE_NULL => AttributeValue::Null,
        TYPE_STRING => AttributeValue::String(utf(i)?),
        TYPE_STRING_INTERNED => AttributeValue::String(String::from(interned(i, pool)?)),
        TYPE_BYTES_HEX => AttributeValue::BytesHex(bytes_blob(i)?),
        TYPE_BYTES_BASE64 => AttributeValue::BytesBase64(bytes_blob(i)?),
        TYPE_INT => AttributeValue::Int(be_i32(i)?),
        TYPE_INT_HEX => AttributeValue::IntHex(be_i32(i)? as u32),
        TYPE_LONG => AttributeValue::Long(be_i64(i)?),
        TYPE_LONG_HEX => AttributeValue::LongHex(be_i64(i)? as u64),
        TYPE_FLOAT => AttributeValue::Float(be_f32(i)?),
        TYPE_DOUBLE => AttributeValue::Double(be_f64(i)?),
        TYPE_BOOLEAN_TRUE => AttributeValue::Boolean(true),
        TYPE_BOOLEAN_FALSE => AttributeValue::Boolean(false),
        other => return fail(AbxError::UnknownAttributeType(other)),
    })
}

fn text<'i, I: AbxInput<'i>>(i: &mut I, type_nibble: u8) -> PResult<String> {
    if type_nibble == TYPE_STRING {
        utf(i)
    } else {
        Ok(String::new())
    }
}

/// Parses one event. The caller handles end of input before calling this.
///
/// On a partial input, a trailing attribute-less start tag reports
/// `Incomplete` until more bytes or EOF decide whether an attribute follows.
pub(crate) fn event<'i, I: AbxInput<'i>>(i: &mut I, pool: &mut Vec<InternedStr>) -> PResult<Event> {
    let token = be_u8(i)?;
    let type_nibble = token & 0xF0;

    Ok(match token & 0x0F {
        CMD_START_DOCUMENT => Event::StartDocument,
        CMD_END_DOCUMENT => Event::EndDocument,
        CMD_START_TAG => {
            let name = interned(i, pool)?;
            let mut attributes = Vec::with_capacity(4);
            loop {
                let next = match i.peek_token() {
                    Some(t) if t & 0x0F == CMD_ATTRIBUTE => t,
                    Some(_) => break,
                    None if i.is_partial() => return Err(ErrMode::Incomplete(Needed::new(1))),
                    None => break,
                };
                i.next_token();
                let name = interned(i, pool)?;
                let value = attr_value(i, pool, next & 0xF0)?;
                attributes.push(Attribute { name, value });
            }
            Event::StartTag { name, attributes }
        }
        CMD_END_TAG => Event::EndTag {
            name: interned(i, pool)?,
        },
        CMD_TEXT => Event::Text(text(i, type_nibble)?),
        CMD_CDSECT => Event::CdataSection(text(i, type_nibble)?),
        CMD_ENTITY_REF => Event::EntityReference(text(i, type_nibble)?),
        CMD_IGNORABLE_WHITESPACE => Event::IgnorableWhitespace(text(i, type_nibble)?),
        CMD_PROCESSING_INSTRUCTION => Event::ProcessingInstruction(text(i, type_nibble)?),
        CMD_COMMENT => Event::Comment(text(i, type_nibble)?),
        CMD_DOCDECL => Event::DocDecl(text(i, type_nibble)?),
        other => return fail(AbxError::UnknownCommand(other)),
    })
}

pub(crate) fn into_abx_error(e: ErrMode<DecodeError>) -> AbxError {
    match e {
        ErrMode::Backtrack(DecodeError(e)) | ErrMode::Cut(DecodeError(e)) => e,
        ErrMode::Incomplete(_) => AbxError::UnexpectedEof("primitive"),
    }
}
