//! Replication protocol framing and pgoutput binary decoder.
//!
//! Two layers of protocol live here:
//! 1. Replication framing — `XLogData` (tag `w`) and `KeepAlive` (tag `k`)
//! 2. pgoutput messages — `Begin`, `Commit`, `Relation`, `Insert`, `Update`, `Delete`

use bytes::{Buf, BufMut, BytesMut};

use crate::error::CdcError;

/// Log Sequence Number — position in the WAL.
pub type Lsn = u64;

// =============================================================================
// Layer 1: Replication stream framing
// =============================================================================

/// A message from the replication stream (inside CopyData).
pub(crate) enum ReplicationMessage {
    /// WAL data containing a pgoutput message.
    XLogData { wal_end: Lsn, data: bytes::Bytes },
    /// Server heartbeat; we must reply if requested.
    KeepAlive { wal_end: Lsn, reply_requested: bool },
}

impl ReplicationMessage {
    /// Parse from the raw bytes inside a CopyData message.
    ///
    /// # Errors
    ///
    /// Returns `CdcError::Protocol` if the message is malformed.
    pub fn parse(mut buf: bytes::Bytes) -> Result<Self, CdcError> {
        if buf.is_empty() {
            return Err(CdcError::Protocol("empty replication message".into()));
        }
        match buf.get_u8() {
            b'w' => {
                ensure_remaining(&buf, 24, "XLogData")?;
                let _wal_start = buf.get_u64();
                let wal_end = buf.get_u64();
                let _server_time = buf.get_i64();
                Ok(Self::XLogData { wal_end, data: buf })
            }
            b'k' => {
                ensure_remaining(&buf, 17, "KeepAlive")?;
                let wal_end = buf.get_u64();
                let _server_time = buf.get_i64();
                let reply_requested = buf.get_u8() != 0;
                Ok(Self::KeepAlive {
                    wal_end,
                    reply_requested,
                })
            }
            tag => Err(CdcError::Protocol(format!(
                "unknown replication tag: 0x{tag:02x}"
            ))),
        }
    }
}

/// Build a `StandbyStatusUpdate` message to send inside CopyData.
pub(crate) fn build_status_update(lsn: Lsn) -> bytes::Bytes {
    let mut buf = BytesMut::with_capacity(34);
    buf.put_u8(b'r');
    buf.put_u64(lsn); // write LSN
    buf.put_u64(lsn); // flush LSN
    buf.put_u64(lsn); // apply LSN
    buf.put_i64(0); // client timestamp (unused by server)
    buf.put_u8(0); // don't request reply
    buf.freeze()
}

// =============================================================================
// Layer 2: pgoutput logical replication messages
// =============================================================================

/// A decoded pgoutput message.
#[derive(Debug)]
pub(crate) enum PgOutputMessage {
    Begin,
    Commit,
    Relation(RelationInfo),
    Insert {
        relation_id: u32,
        tuple: Vec<TupleValue>,
    },
    Update {
        relation_id: u32,
        new_tuple: Vec<TupleValue>,
    },
    Delete,
}

/// Schema info for a replicated table, received before any DML for that table.
#[derive(Debug, Clone)]
pub(crate) struct RelationInfo {
    pub id: u32,
    pub name: String,
    pub columns: Vec<ColumnInfo>,
}

/// Column metadata from a Relation message.
#[derive(Debug, Clone)]
pub(crate) struct ColumnInfo {
    pub name: String,
}

/// A single column value from a replicated tuple.
#[derive(Debug, Clone)]
pub(crate) enum TupleValue {
    Null,
    UnchangedToast,
    Text(String),
}

impl PgOutputMessage {
    /// Parse a pgoutput message from the XLogData payload.
    ///
    /// Returns `Ok(None)` for message types we don't handle (Truncate, Origin, etc.).
    ///
    /// # Errors
    ///
    /// Returns `CdcError::Protocol` for malformed messages.
    pub fn parse(mut buf: &[u8]) -> Result<Option<Self>, CdcError> {
        if buf.is_empty() {
            return Err(CdcError::Protocol("empty pgoutput message".into()));
        }
        let tag = buf.get_u8();
        match tag {
            b'B' => {
                // final_lsn(8) + commit_time(8) + xid(4) = 20 bytes
                ensure_remaining_slice(buf, 20, "Begin")?;
                buf.advance(20);
                Ok(Some(Self::Begin))
            }
            b'C' => {
                // flags(1) + commit_lsn(8) + end_lsn(8) + commit_time(8) = 25 bytes
                ensure_remaining_slice(buf, 25, "Commit")?;
                buf.advance(25);
                Ok(Some(Self::Commit))
            }
            b'R' => parse_relation(&mut buf).map(Some),
            b'I' => {
                ensure_remaining_slice(buf, 5, "Insert")?;
                let relation_id = buf.get_u32();
                let _new_tag = buf.get_u8(); // 'N'
                let tuple = parse_tuple(&mut buf)?;
                Ok(Some(Self::Insert { relation_id, tuple }))
            }
            b'U' => {
                ensure_remaining_slice(buf, 5, "Update")?;
                let relation_id = buf.get_u32();
                let next = buf.get_u8();
                if next == b'K' || next == b'O' {
                    // Old tuple — skip it
                    let _old = parse_tuple(&mut buf)?;
                    if buf.has_remaining() {
                        let _new_tag = buf.get_u8(); // 'N'
                    }
                }
                let new_tuple = parse_tuple(&mut buf)?;
                Ok(Some(Self::Update {
                    relation_id,
                    new_tuple,
                }))
            }
            b'D' => {
                // We don't process deletes for prices/FX — just skip the payload
                Ok(Some(Self::Delete))
            }
            // Truncate ('T'), Origin ('O'), Type ('Y'), Message ('M')
            _ => Ok(None),
        }
    }
}

fn parse_relation(buf: &mut &[u8]) -> Result<PgOutputMessage, CdcError> {
    ensure_remaining_slice(buf, 4, "Relation")?;
    let id = buf.get_u32();
    let _namespace = read_cstring(buf)?;
    let name = read_cstring(buf)?;
    ensure_remaining_slice(buf, 3, "Relation columns")?;
    let _replica_identity = buf.get_u8();
    let num_columns = buf.get_i16();

    let mut columns = Vec::with_capacity(num_columns.max(0) as usize);
    for _ in 0..num_columns {
        ensure_remaining_slice(buf, 1, "column flags")?;
        let _flags = buf.get_u8();
        let col_name = read_cstring(buf)?;
        ensure_remaining_slice(buf, 8, "column type")?;
        let _type_oid = buf.get_u32();
        let _type_mod = buf.get_i32();
        columns.push(ColumnInfo { name: col_name });
    }

    Ok(PgOutputMessage::Relation(RelationInfo {
        id,
        name,
        columns,
    }))
}

#[allow(clippy::cast_sign_loss)]
fn parse_tuple(buf: &mut &[u8]) -> Result<Vec<TupleValue>, CdcError> {
    ensure_remaining_slice(buf, 2, "tuple header")?;
    let num_cols = buf.get_i16();
    let mut values = Vec::with_capacity(num_cols.max(0) as usize);
    for _ in 0..num_cols {
        ensure_remaining_slice(buf, 1, "tuple column tag")?;
        match buf.get_u8() {
            b'n' => values.push(TupleValue::Null),
            b'u' => values.push(TupleValue::UnchangedToast),
            b't' => {
                ensure_remaining_slice(buf, 4, "text length")?;
                let len = buf.get_i32() as usize;
                if buf.remaining() < len {
                    return Err(CdcError::Protocol("tuple text data truncated".into()));
                }
                let text = String::from_utf8_lossy(&buf.chunk()[..len]).into_owned();
                buf.advance(len);
                values.push(TupleValue::Text(text));
            }
            b'b' => {
                // Binary format — skip (we request text via pgoutput)
                ensure_remaining_slice(buf, 4, "binary length")?;
                let len = buf.get_i32() as usize;
                buf.advance(len.min(buf.remaining()));
                values.push(TupleValue::Null);
            }
            tag => {
                return Err(CdcError::Protocol(format!(
                    "unknown tuple tag: 0x{tag:02x}"
                )));
            }
        }
    }
    Ok(values)
}

/// Read a null-terminated C string from the buffer.
fn read_cstring(buf: &mut &[u8]) -> Result<String, CdcError> {
    let pos = buf
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| CdcError::Protocol("unterminated string".into()))?;
    let s = String::from_utf8_lossy(&buf[..pos]).into_owned();
    buf.advance(pos + 1);
    Ok(s)
}

fn ensure_remaining(buf: &impl Buf, needed: usize, context: &str) -> Result<(), CdcError> {
    if buf.remaining() < needed {
        return Err(CdcError::Protocol(format!(
            "{context}: need {needed} bytes, have {}",
            buf.remaining()
        )));
    }
    Ok(())
}

fn ensure_remaining_slice(buf: &[u8], needed: usize, context: &str) -> Result<(), CdcError> {
    if buf.len() < needed {
        return Err(CdcError::Protocol(format!(
            "{context}: need {needed} bytes, have {}",
            buf.len()
        )));
    }
    Ok(())
}
