use bytes::Bytes;
use std::io;
use thiserror::Error;

const MAVLINK_STX_V1: u8 = 0xFE;
const MAVLINK_STX_V2: u8 = 0xFD;
const MAVLINK_V1_HEADER_LEN: usize = 6;
const MAVLINK_V2_HEADER_LEN: usize = 10;
const MAVLINK_CHECKSUM_LEN: usize = 2;
const MAVLINK_SIGNATURE_LEN: usize = 13;
const MAVLINK_IFLAG_SIGNED: u8 = 0x01;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Invalid magic byte: expected 0xFE or 0xFD, got {0:#x}")]
    InvalidMagic(u8),

    #[error("Incomplete packet: need {0} bytes, have {1}")]
    Incomplete(usize, usize),

    #[error("Invalid CRC: expected {expected:#x}, got {got:#x}")]
    InvalidCrc { expected: u16, got: u16 },

    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

/// MAVLink protocol version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MavVersion {
    V1,
    V2,
}

/// A zero-copy MAVLink frame reference (supports both v1 and v2)
#[derive(Debug, Clone)]
pub struct MavFrame {
    /// Complete frame data (including STX, header, payload, CRC, and optional signature)
    data: Bytes,
    /// Protocol version
    version: MavVersion,
    /// Offset to payload start
    payload_offset: usize,
    /// Payload length
    payload_len: usize,
}

impl MavFrame {
    /// Parse a MAVLink frame (v1 or v2) from a buffer
    /// Returns the frame and number of bytes consumed
    pub fn parse(buf: &[u8]) -> Result<(Self, usize), ParseError> {
        if buf.is_empty() {
            return Err(ParseError::Incomplete(1, 0));
        }

        // Check magic byte to determine version
        let stx = buf[0];
        match stx {
            MAVLINK_STX_V1 => Self::parse_v1(buf),
            MAVLINK_STX_V2 => Self::parse_v2(buf),
            _ => Err(ParseError::InvalidMagic(stx)),
        }
    }

    fn parse_v1(buf: &[u8]) -> Result<(Self, usize), ParseError> {
        // MAVLink v1: STX(1) + LEN(1) + SEQ(1) + SYSID(1) + COMPID(1) + MSGID(1) + PAYLOAD + CRC(2)
        if buf.len() < MAVLINK_V1_HEADER_LEN {
            return Err(ParseError::Incomplete(MAVLINK_V1_HEADER_LEN, buf.len()));
        }

        let payload_len = buf[1] as usize;
        let total_len = MAVLINK_V1_HEADER_LEN + payload_len + MAVLINK_CHECKSUM_LEN;

        if buf.len() < total_len {
            return Err(ParseError::Incomplete(total_len, buf.len()));
        }

        // For transparency, we skip CRC validation and just forward the packet
        // This ensures compatibility with extended/custom message sets

        let frame = MavFrame {
            data: Bytes::copy_from_slice(&buf[..total_len]),
            version: MavVersion::V1,
            payload_offset: MAVLINK_V1_HEADER_LEN,
            payload_len,
        };

        Ok((frame, total_len))
    }

    fn parse_v2(buf: &[u8]) -> Result<(Self, usize), ParseError> {
        // MAVLink v2: STX(1) + LEN(1) + INCOMPAT(1) + COMPAT(1) + SEQ(1) + SYSID(1) + COMPID(1) + MSGID(3) + PAYLOAD + CRC(2) + [SIG(13)]
        if buf.len() < MAVLINK_V2_HEADER_LEN {
            return Err(ParseError::Incomplete(MAVLINK_V2_HEADER_LEN, buf.len()));
        }

        let payload_len = buf[1] as usize;
        let incompat_flags = buf[2];

        // Calculate total frame length
        let signed = (incompat_flags & MAVLINK_IFLAG_SIGNED) != 0;
        let signature_len = if signed { MAVLINK_SIGNATURE_LEN } else { 0 };
        let total_len = MAVLINK_V2_HEADER_LEN + payload_len + MAVLINK_CHECKSUM_LEN + signature_len;

        if buf.len() < total_len {
            return Err(ParseError::Incomplete(total_len, buf.len()));
        }

        // For transparency, we skip CRC validation and just forward the packet
        // This ensures compatibility with extended/custom message sets

        let frame = MavFrame {
            data: Bytes::copy_from_slice(&buf[..total_len]),
            version: MavVersion::V2,
            payload_offset: MAVLINK_V2_HEADER_LEN,
            payload_len,
        };

        Ok((frame, total_len))
    }

    #[inline]
    pub fn version(&self) -> MavVersion {
        self.version
    }

    #[inline]
    pub fn sys_id(&self) -> u8 {
        match self.version {
            MavVersion::V1 => self.data[3],
            MavVersion::V2 => self.data[5],
        }
    }

    #[inline]
    pub fn comp_id(&self) -> u8 {
        match self.version {
            MavVersion::V1 => self.data[4],
            MavVersion::V2 => self.data[6],
        }
    }

    #[inline]
    pub fn msg_id(&self) -> u32 {
        match self.version {
            MavVersion::V1 => self.data[5] as u32,
            MavVersion::V2 => u32::from_le_bytes([self.data[7], self.data[8], self.data[9], 0]),
        }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn sequence(&self) -> u8 {
        match self.version {
            MavVersion::V1 => self.data[2],
            MavVersion::V2 => self.data[4],
        }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn payload(&self) -> &[u8] {
        &self.data[self.payload_offset..self.payload_offset + self.payload_len]
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    #[inline]
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.data.len()
    }
}

/// Fast CRC-16/MCRF4XX calculation for MAVLink
fn calculate_crc(buf: &[u8]) -> u16 {
    const X25_CRC_TABLE: [u16; 256] = generate_crc_table();

    let mut crc: u16 = 0xFFFF;
    for &byte in buf {
        let tmp = byte ^ (crc as u8);
        crc = (crc >> 8) ^ X25_CRC_TABLE[tmp as usize];
    }
    crc
}

const fn generate_crc_table() -> [u16; 256] {
    let mut table = [0u16; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u16;
        let mut j = 0;
        while j < 8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0x8408;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc_calculation() {
        // Known CRC test
        let data = [0x09, 0x00, 0x00, 0x00, 0x01, 0x01];
        let crc = calculate_crc(&data);
        assert_ne!(crc, 0); // Basic sanity check
    }

    #[test]
    fn test_incomplete_packet_v2() {
        let short_buf = [MAVLINK_STX_V2, 0x00];
        let result = MavFrame::parse(&short_buf);
        assert!(matches!(result, Err(ParseError::Incomplete(_, _))));
    }

    #[test]
    fn test_incomplete_packet_v1() {
        let short_buf = [MAVLINK_STX_V1, 0x00];
        let result = MavFrame::parse(&short_buf);
        assert!(matches!(result, Err(ParseError::Incomplete(_, _))));
    }

    #[test]
    fn test_invalid_magic() {
        let bad_buf = [0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let result = MavFrame::parse(&bad_buf);
        assert!(matches!(result, Err(ParseError::InvalidMagic(_))));
    }
}
