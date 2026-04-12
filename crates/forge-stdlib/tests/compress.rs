use forge_stdlib::compress::{compress_str, decompress_str, CompressAlgo};

#[test]
fn test_gzip_compress_decompress_roundtrip() {
    let input = "forge ".repeat(64);
    let compressed = compress_str(&input, CompressAlgo::Gzip).expect("gzip should compress");
    let decompressed =
        decompress_str(&compressed, CompressAlgo::Gzip).expect("gzip should decompress");
    assert_eq!(decompressed, input);
}

#[test]
fn test_gzip_compressed_smaller_than_input() {
    let input = "aaaaabbbbbcccccdddddeeeee".repeat(64);
    let compressed = compress_str(&input, CompressAlgo::Gzip).expect("gzip should compress");
    assert!(compressed.len() < input.len());
}

#[test]
fn test_gzip_decompress_invalid_errors() {
    let err = decompress_str(b"not gzip data", CompressAlgo::Gzip).expect_err("should fail");
    assert!(err.contains("gzip decompress failed"), "got: {}", err);
}

#[test]
fn test_brotli_compress_decompress_roundtrip() {
    let input = "{\"items\":[\"forge\",\"stdlib\",\"brotli\"]}".repeat(64);
    let compressed = compress_str(&input, CompressAlgo::Brotli).expect("brotli should compress");
    let decompressed =
        decompress_str(&compressed, CompressAlgo::Brotli).expect("brotli should decompress");
    assert_eq!(decompressed, input);
}

#[test]
fn test_brotli_better_ratio_than_gzip() {
    let input =
        "{\"name\":\"forge\",\"kind\":\"stdlib\",\"payload\":\"aaaaaaaaaabbbbbbbbbbcccccccccc\"}"
            .repeat(128);
    let gzip = compress_str(&input, CompressAlgo::Gzip).expect("gzip should compress");
    let brotli = compress_str(&input, CompressAlgo::Brotli).expect("brotli should compress");
    assert!(
        brotli.len() < gzip.len(),
        "brotli={} gzip={}",
        brotli.len(),
        gzip.len()
    );
}
