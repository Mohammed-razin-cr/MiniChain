pub fn required_quorum(active_validators: usize) -> usize {
    if active_validators == 0 {
        0
    } else {
        (active_validators * 2) / 3 + 1
    }
}
