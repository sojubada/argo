//! 옛 파워포인트(.ppt) 읽기. (뒤에 채운다)

pub(crate) fn is_ppt(_bytes: &[u8]) -> bool {
    false
}

pub(crate) fn convert(_bytes: &[u8]) -> Result<Vec<u8>, String> {
    Err("아직 ppt 를 못 읽는다".into())
}
