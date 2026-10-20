#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    Loading,
    Available,
    UnsupportedHardware,
    MissingService,
    PermissionDenied,
    Disconnected,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_states_are_distinct_without_implicit_fallback() {
        let states = [
            Availability::Loading,
            Availability::Available,
            Availability::UnsupportedHardware,
            Availability::MissingService,
            Availability::PermissionDenied,
            Availability::Disconnected,
            Availability::Failed,
        ];
        for (index, state) in states.iter().enumerate() {
            assert_eq!(*state, states[index]);
            for other in &states[index + 1..] {
                assert_ne!(state, other);
            }
        }
    }
}
