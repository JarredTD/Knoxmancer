//! PNG preview generation and structural validation.

const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// Generates a valid RGBA PNG with an uncompressed zlib stream.
pub(crate) fn generate(width: u32, height: u32) -> Vec<u8> {
    let mut raw = Vec::with_capacity(height as usize * (1 + width as usize * 4));
    let mut row = Vec::with_capacity(1 + width as usize * 4);
    row.push(0);
    for _ in 0..width {
        row.extend_from_slice(&[44, 48, 46, 255]);
    }
    for _ in 0..height {
        raw.extend_from_slice(&row);
    }

    let mut compressed = vec![0x78, 0x01];
    let mut remaining = raw.as_slice();
    while !remaining.is_empty() {
        let length = remaining.len().min(65_535);
        let final_block = length == remaining.len();
        compressed.push(u8::from(final_block));
        compressed.extend_from_slice(&(length as u16).to_le_bytes());
        compressed.extend_from_slice(&(!(length as u16)).to_le_bytes());
        compressed.extend_from_slice(&remaining[..length]);
        remaining = &remaining[length..];
    }
    compressed.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::new();
    png.extend_from_slice(SIGNATURE);
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    write_chunk(&mut png, b"IHDR", &header);
    write_chunk(&mut png, b"IDAT", &compressed);
    write_chunk(&mut png, b"IEND", &[]);
    png
}

/// Validates PNG framing, critical chunks, checksums, and header fields.
pub(crate) fn inspect(data: &[u8]) -> std::result::Result<(u32, u32), String> {
    if !data.starts_with(SIGNATURE) {
        return Err("invalid PNG signature".to_owned());
    }
    let mut offset = SIGNATURE.len();
    let mut dimensions = None;
    let mut saw_data = false;
    let mut saw_end = false;
    while offset < data.len() {
        let header_end = offset
            .checked_add(8)
            .filter(|end| *end <= data.len())
            .ok_or_else(|| "truncated PNG chunk header".to_owned())?;
        let length = u32::from_be_bytes(
            data[offset..offset + 4]
                .try_into()
                .expect("four-byte chunk length"),
        ) as usize;
        let kind: [u8; 4] = data[offset + 4..header_end]
            .try_into()
            .expect("four-byte chunk type");
        let data_end = header_end
            .checked_add(length)
            .filter(|end| end.saturating_add(4) <= data.len())
            .ok_or_else(|| "truncated PNG chunk data".to_owned())?;
        let checksum_end = data_end + 4;
        let expected = u32::from_be_bytes(
            data[data_end..checksum_end]
                .try_into()
                .expect("four-byte chunk checksum"),
        );
        if crc32(&data[offset + 4..data_end]) != expected {
            return Err(format!(
                "invalid {} chunk checksum",
                String::from_utf8_lossy(&kind)
            ));
        }
        let chunk = &data[header_end..data_end];
        match &kind {
            b"IHDR" => {
                if dimensions.is_some() || offset != SIGNATURE.len() || chunk.len() != 13 {
                    return Err("invalid IHDR chunk".to_owned());
                }
                let width = u32::from_be_bytes(chunk[0..4].try_into().expect("width"));
                let height = u32::from_be_bytes(chunk[4..8].try_into().expect("height"));
                let valid_color_depth = matches!(
                    (chunk[9], chunk[8]),
                    (0, 1 | 2 | 4 | 8 | 16) | (2, 8 | 16) | (3, 1 | 2 | 4 | 8) | (4 | 6, 8 | 16)
                );
                if width == 0
                    || height == 0
                    || !valid_color_depth
                    || chunk[10] != 0
                    || chunk[11] != 0
                    || chunk[12] > 1
                {
                    return Err("unsupported PNG header".to_owned());
                }
                dimensions = Some((width, height));
            }
            b"IDAT" => saw_data = true,
            b"IEND" => {
                if !chunk.is_empty() || checksum_end != data.len() {
                    return Err("invalid IEND chunk".to_owned());
                }
                saw_end = true;
            }
            b"PLTE" => {}
            _ if kind[0].is_ascii_uppercase() => {
                return Err(format!(
                    "unsupported critical PNG chunk {}",
                    String::from_utf8_lossy(&kind)
                ));
            }
            _ => {}
        }
        offset = checksum_end;
    }
    if !saw_data || !saw_end {
        return Err("PNG requires IDAT and IEND chunks".to_owned());
    }
    dimensions.ok_or_else(|| "PNG requires an IHDR chunk".to_owned())
}

fn write_chunk(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(data);
    output.extend_from_slice(&crc32(&output[output.len() - data.len() - 4..]).to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in data {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_and_inspects_pngs() {
        let image = generate(256, 256);
        assert_eq!(inspect(&image).unwrap(), (256, 256));
        let mut corrupt = image;
        corrupt[20] ^= 1;
        assert!(inspect(&corrupt).is_err());
        assert!(inspect(b"not png").is_err());
    }
}
