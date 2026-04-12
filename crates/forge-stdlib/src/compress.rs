use std::io::{Read, Write};

use brotli::{CompressorReader, Decompressor};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressAlgo {
    Gzip,
    Brotli,
}

pub fn compress(input: &[u8], algo: CompressAlgo) -> Result<Vec<u8>, String> {
    match algo {
        CompressAlgo::Gzip => gzip_compress(input),
        CompressAlgo::Brotli => brotli_compress(input),
    }
}

pub fn compress_str(input: impl AsRef<str>, algo: CompressAlgo) -> Result<Vec<u8>, String> {
    compress(input.as_ref().as_bytes(), algo)
}

pub fn decompress(input: &[u8], algo: CompressAlgo) -> Result<Vec<u8>, String> {
    match algo {
        CompressAlgo::Gzip => gzip_decompress(input),
        CompressAlgo::Brotli => brotli_decompress(input),
    }
}

pub fn decompress_str(input: &[u8], algo: CompressAlgo) -> Result<String, String> {
    let bytes = decompress(input, algo)?;
    String::from_utf8(bytes)
        .map_err(|err| format!("decompressed payload is not valid utf-8: {}", err))
}

fn gzip_compress(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(input)
        .map_err(|err| format!("gzip compress failed: {}", err))?;
    encoder
        .finish()
        .map_err(|err| format!("gzip finalize failed: {}", err))
}

fn gzip_decompress(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = GzDecoder::new(input);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|err| format!("gzip decompress failed: {}", err))?;
    Ok(out)
}

fn brotli_compress(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = CompressorReader::new(input, 4096, 5, 22);
    let mut out = Vec::new();
    reader
        .read_to_end(&mut out)
        .map_err(|err| format!("brotli compress failed: {}", err))?;
    Ok(out)
}

fn brotli_decompress(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = Decompressor::new(input, 4096);
    let mut out = Vec::new();
    reader
        .read_to_end(&mut out)
        .map_err(|err| format!("brotli decompress failed: {}", err))?;
    Ok(out)
}
