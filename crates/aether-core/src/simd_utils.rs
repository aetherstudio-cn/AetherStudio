//! SIMD加速的文本处理工具
//!
//! P0-D: 换行计数与字节查找委托给 bytecount / memchr（内置 AVX2/SSE2
//! 运行时分派，比手写 SWAR 快 4-8 倍）；空白跳过保留 SWAR 实现但改用
//! chunks_exact 消除逐字节索引的边界检查，便于编译器向量化。
//! 函数签名保持不变，调用方零改动。

/// 快速计算字节数组中的换行符数量（bytecount：AVX2 运行时分派）
#[inline]
pub fn count_newlines_simd(data: &[u8]) -> u32 {
    bytecount::count(data, b'\n') as u32
}

/// 快速查找字节在数组中的位置（memchr：AVX2 运行时分派）
#[inline]
pub fn find_byte_simd(data: &[u8], target: u8) -> Option<usize> {
    memchr::memchr(target, data)
}

/// 快速跳过空白字符（空格、制表符、回车）
///
/// 16 字节 SWAR 批量检测；使用 chunks_exact 让编译器生成无边界检查的向量加载
pub fn skip_whitespace_simd(data: &[u8], start: usize) -> usize {
    let len = data.len();
    let mut i = start;

    // 16 字节批量检测：整块全为空白才整块跳过
    for chunk in data[start.min(len)..].chunks_exact(16) {
        let v = u128::from_le_bytes(chunk.try_into().unwrap());

        let is_space = v ^ 0x20202020202020202020202020202020u128;
        let is_tab = v ^ 0x09090909090909090909090909090909u128;
        let is_cr = v ^ 0x0D0D0D0D0D0D0D0D0D0D0D0D0D0D0D0Du128;

        let is_whitespace =
            has_zero_byte_u128(is_space) | has_zero_byte_u128(is_tab) | has_zero_byte_u128(is_cr);

        if is_whitespace != 0x80808080808080808080808080808080u128 {
            // 不是所有字节都是空白，退出批量路径逐个处理
            break;
        }

        i += 16;
    }

    // 逐个处理剩余字节
    while i < len {
        match data[i] {
            b' ' | b'\t' | b'\r' => i += 1,
            _ => break,
        }
    }

    i
}

/// 检测 128 位整数中是否有 0 字节
#[inline(always)]
fn has_zero_byte_u128(x: u128) -> u128 {
    let sub = x.wrapping_sub(0x01010101010101010101010101010101u128);
    let not_x = !x;
    sub & not_x & 0x80808080808080808080808080808080u128
}

/// 快速字符串前缀匹配（用于关键字检测）
///
/// 前缀通常很短（关键字 2-8 字节），切片比较由标准库向量化
#[inline]
pub fn starts_with_simd(data: &[u8], prefix: &[u8]) -> bool {
    data.len() >= prefix.len() && &data[..prefix.len()] == prefix
}

/// 快速计算字符串长度（到下一个换行符）
pub fn line_length_simd(data: &[u8], start: usize) -> usize {
    match find_byte_simd(&data[start..], b'\n') {
        Some(pos) => pos,
        None => data.len() - start,
    }
}

/// 批量检测字符类型（用于lexer）
///
/// 返回每个字节的字符类型分类
/// 类型：0=其他, 1=字母, 2=数字, 3=空白
#[allow(dead_code)]
pub fn classify_chars_simd(data: &[u8], start: usize, out: &mut [u8]) {
    let len = data.len().saturating_sub(start).min(out.len());
    for (o, &b) in out[..len].iter_mut().zip(&data[start..start + len]) {
        *o = classify_byte(b);
    }
}

#[inline(always)]
fn classify_byte(byte: u8) -> u8 {
    match byte {
        b'a'..=b'z' | b'A'..=b'Z' | b'_' => 1, // 字母/标识符
        b'0'..=b'9' => 2,                      // 数字
        b' ' | b'\t' | b'\r' | b'\n' => 3,     // 空白
        _ => 0,                                // 其他
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_newlines_simd() {
        let data = b"line1\nline2\nline3\n";
        assert_eq!(count_newlines_simd(data), 3);

        let data2 = b"no newlines here";
        assert_eq!(count_newlines_simd(data2), 0);

        let data3 = b"\n\n\n";
        assert_eq!(count_newlines_simd(data3), 3);
    }

    #[test]
    fn test_find_byte_simd() {
        let data = b"hello world\nfoo";
        assert_eq!(find_byte_simd(data, b'\n'), Some(11));
        assert_eq!(find_byte_simd(data, b'x'), None);
        assert_eq!(find_byte_simd(data, b'h'), Some(0));
    }

    #[test]
    fn test_find_byte_simd_non_ascii() {
        // C-01: 验证高字节（CJK、带音标字符）不会误报
        // 中(3)文(3)测(3)试(3)内(3)容(3) = 18 字节，\n 位于索引 18
        let data = "中文测试内容\n下一行".as_bytes();
        assert_eq!(find_byte_simd(data, b'\n'), Some(18));

        // café(5) space(1) résumé(8) space(1) naïve(6) \n(1) = 22 字节，\n 位于索引 21
        let data2 = "café résumé naïve\n".as_bytes();
        assert_eq!(find_byte_simd(data2, b'\n'), Some(21));

        // 全高字节无目标字节时应返回 None
        let data3 = "🎉 emoji 测试 🚀".as_bytes();
        assert_eq!(find_byte_simd(data3, b'\n'), None);
    }

    #[test]
    fn test_skip_whitespace_simd() {
        let data = b"   \t\t  hello";
        assert_eq!(skip_whitespace_simd(data, 0), 7);

        let data2 = b"hello";
        assert_eq!(skip_whitespace_simd(data2, 0), 0);
    }

    #[test]
    fn test_starts_with_simd() {
        assert!(starts_with_simd(b"hello world", b"hello"));
        assert!(!starts_with_simd(b"hello world", b"world"));
        assert!(starts_with_simd(b"fn main()", b"fn"));
    }

    #[test]
    fn test_large_file_newlines() {
        // 测试大文件场景
        let mut data = Vec::with_capacity(10000);
        for i in 0..1000 {
            data.extend_from_slice(format!("line {}\n", i).as_bytes());
        }

        let simd_count = count_newlines_simd(&data);
        let scalar_count = data.iter().filter(|&&b| b == b'\n').count() as u32;
        assert_eq!(simd_count, scalar_count);
    }

    #[test]
    fn test_16byte_boundary() {
        // 测试 16 字节边界情况
        let data = b"0123456789abcdef\nmore";
        assert_eq!(find_byte_simd(data, b'\n'), Some(16));

        let data2 = b"0123456789abcde\nmore";
        assert_eq!(find_byte_simd(data2, b'\n'), Some(15));

        let data3 = b"0123456789abcdefg\nmore";
        assert_eq!(find_byte_simd(data3, b'\n'), Some(17));
    }

    #[test]
    fn test_count_newlines_large() {
        // 测试大数据（> 32 字节，确保向量路径生效）
        let mut data = vec![b'a'; 128];
        data[15] = b'\n';
        data[31] = b'\n';
        data[63] = b'\n';
        data[127] = b'\n';
        assert_eq!(count_newlines_simd(&data), 4);
    }

    #[test]
    fn test_count_newlines_empty_and_small() {
        assert_eq!(count_newlines_simd(b""), 0);
        assert_eq!(count_newlines_simd(b"\n"), 1);
        assert_eq!(count_newlines_simd(b"abc"), 0);
    }

    #[test]
    fn test_count_newlines_8byte_boundary() {
        // 短数据路径：长度 8-15
        let data = b"abcdefg\n";
        assert_eq!(count_newlines_simd(data), 1);
        let data2 = b"abc\ndef\n";
        assert_eq!(count_newlines_simd(data2), 2);
    }

    #[test]
    fn test_find_byte_simd_boundaries() {
        assert_eq!(find_byte_simd(b"", b'x'), None);
        assert_eq!(find_byte_simd(b"x", b'x'), Some(0));
        assert_eq!(find_byte_simd(b"abcdefghijklmnopq", b'q'), Some(16));
        assert_eq!(find_byte_simd(b"abcdefghijklmnop", b'x'), None);
    }

    #[test]
    fn test_skip_whitespace_simd_boundaries() {
        assert_eq!(skip_whitespace_simd(b"", 0), 0);
        assert_eq!(skip_whitespace_simd(b"hello", 0), 0);
        assert_eq!(skip_whitespace_simd(b"   hello", 3), 3);
        assert_eq!(skip_whitespace_simd(b"            x", 0), 12); // 12 个空白
        assert_eq!(skip_whitespace_simd(b"                x", 0), 16); // 16 个空白
    }

    #[test]
    fn test_starts_with_simd_boundaries() {
        assert!(starts_with_simd(b"hello", b""));
        assert!(starts_with_simd(b"hello", b"hello"));
        assert!(!starts_with_simd(b"hi", b"hello"));
        assert!(starts_with_simd(b"hello world", b"hello wo"));
        assert!(starts_with_simd(b"longer prefix here", b"longer prefix"));
    }

    #[test]
    fn test_line_length_simd() {
        assert_eq!(line_length_simd(b"hello\nworld", 0), 5);
        assert_eq!(line_length_simd(b"hello world", 0), 11);
        assert_eq!(line_length_simd(b"a\nb", 2), 1);
    }

    #[test]
    fn test_classify_chars_simd() {
        let data = b"a1 _\n";
        let mut out = [0u8; 5];
        classify_chars_simd(data, 0, &mut out);
        assert_eq!(out[0], 1); // letter
        assert_eq!(out[1], 2); // digit
        assert_eq!(out[2], 3); // whitespace
        assert_eq!(out[3], 1); // underscore -> letter
        assert_eq!(out[4], 3); // newline -> whitespace
    }

    #[test]
    fn test_classify_chars_simd_offset() {
        let data = b"xx123";
        let mut out = [0u8; 3];
        classify_chars_simd(data, 2, &mut out);
        assert_eq!(out[0], 2);
        assert_eq!(out[1], 2);
        assert_eq!(out[2], 2);
    }
}
