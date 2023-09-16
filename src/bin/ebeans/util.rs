use std::ascii;

pub(crate) fn bytes_to_human_str(input: &[u8]) -> String {
    String::from_utf8(
        input
            .iter()
            .flat_map(|&c| ascii::escape_default(c))
            .collect::<Vec<u8>>(),
    )
    .unwrap()
}
