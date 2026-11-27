//! Pure core of the polkit authentication agent: the request state machine
//! and the choice of identity to authenticate as. No D-Bus, no X11, no
//! password is ever stored here.

/// One identity polkit offered in `BeginAuthentication`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identity {
    /// A `unix-user` identity: the agent may authenticate as this uid.
    UnixUser { uid: u32 },
    /// Any other identity kind (for example `unix-group`); the agent cannot
    /// authenticate as these, so they are never chosen.
    Other { kind: String },
}

/// Choose the identity to authenticate as.
///
/// The agent's own user wins when polkit offers it (the usual case: a user
/// unlocking their own session action). Otherwise the first `unix-user`
/// identity in polkit's order is used, so administrator accounts such as
/// root are reachable. `None` means polkit offered no identity the agent can
/// use.
pub fn choose_identity(candidates: &[Identity], agent_uid: u32) -> Option<&Identity> {
    let user = |identity: &&Identity| matches!(identity, Identity::UnixUser { .. });
    candidates
        .iter()
        .find(|identity| matches!(identity, Identity::UnixUser { uid } if *uid == agent_uid))
        .or_else(|| candidates.iter().find(user))
}

/// Why a request ended without being answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// polkit offered no identity the agent can authenticate as.
    NoIdentity,
    /// The polkit authentication helper is not installed or cannot run.
    HelperUnavailable,
    /// The helper rejected the password the user typed.
    WrongPassword,
    /// Any other helper or bus error.
    Helper,
}

impl Failure {
    /// The line shown in the dialog's error state.
    pub fn message(self) -> &'static str {
        match self {
            Self::NoIdentity => "No account can be used to authenticate this action.",
            Self::HelperUnavailable => "The polkit authentication helper is not installed.",
            Self::WrongPassword => "Wrong password",
            Self::Helper => "Authentication failed.",
        }
    }
}

/// Lifecycle of one authentication request.
///
/// `Idle` -> `Prompting` (polkit's BeginAuthentication) -> `Verifying` (the
/// user submitted a password; the helper checks it) -> `Answered` (polkit has
/// the authorization) or `Failed`. `Cancelled` can be reached from any live
/// phase. `Answered`, `Cancelled` and `Failed` are terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Prompting,
    Verifying,
    Answered,
    Cancelled,
    Failed(Failure),
}

/// A transition was requested from a phase that does not allow it. The phase
/// is left unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionError {
    pub from: Phase,
    pub action: &'static str,
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot {} from {:?}", self.action, self.from)
    }
}

impl std::error::Error for TransitionError {}

/// One authentication request. Holds no secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRequest {
    pub cookie: String,
    pub action_id: String,
    /// polkit's message, e.g. "Authentication is required to install software".
    pub message: String,
    pub identity: Identity,
    phase: Phase,
    attempt: u32,
    wrong_password_before: bool,
}

impl AuthRequest {
    pub fn new(cookie: &str, action_id: &str, message: &str, identity: Identity) -> Self {
        Self {
            cookie: cookie.to_owned(),
            action_id: action_id.to_owned(),
            message: message.to_owned(),
            identity,
            phase: Phase::Idle,
            attempt: 0,
            wrong_password_before: false,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Which attempt polkit is on for this request (1 for the first prompt).
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Show "Wrong password" in the prompt. Only true for a retry that polkit
    /// started after the helper rejected the previous password.
    pub fn shows_wrong_password(&self) -> bool {
        self.phase == Phase::Prompting && self.wrong_password_before
    }

    /// polkit called BeginAuthentication. `after_wrong_password` says the
    /// previous attempt on this agent was rejected for its password.
    pub fn begin(&mut self, after_wrong_password: bool) -> Result<(), TransitionError> {
        self.step(Phase::Idle, "begin", |state| {
            state.phase = Phase::Prompting;
            state.attempt += 1;
            state.wrong_password_before = after_wrong_password;
        })
    }

    /// The user submitted a password; it is now being verified.
    pub fn submit(&mut self) -> Result<(), TransitionError> {
        self.step(Phase::Prompting, "submit", |state| {
            state.phase = Phase::Verifying;
        })
    }

    /// The helper accepted the password and polkit has the authorization.
    pub fn verified(&mut self) -> Result<(), TransitionError> {
        self.step(Phase::Verifying, "verify", |state| {
            state.phase = Phase::Answered;
        })
    }

    /// The helper rejected the verification.
    pub fn rejected(&mut self, wrong_password: bool) -> Result<(), TransitionError> {
        let failure = if wrong_password {
            Failure::WrongPassword
        } else {
            Failure::Helper
        };
        self.fail(failure, "reject")
    }

    /// The request could not run (no identity, no helper, ...).
    pub fn fail(&mut self, failure: Failure, action: &'static str) -> Result<(), TransitionError> {
        match self.phase {
            Phase::Idle | Phase::Prompting | Phase::Verifying => {
                self.phase = Phase::Failed(failure);
                Ok(())
            }
            from => Err(TransitionError { from, action }),
        }
    }

    /// The user cancelled, or polkit cancelled the request.
    pub fn cancel(&mut self) -> Result<(), TransitionError> {
        match self.phase {
            Phase::Idle | Phase::Prompting | Phase::Verifying => {
                self.phase = Phase::Cancelled;
                Ok(())
            }
            from => Err(TransitionError {
                from,
                action: "cancel",
            }),
        }
    }

    fn step(
        &mut self,
        expected: Phase,
        action: &'static str,
        apply: impl FnOnce(&mut Self),
    ) -> Result<(), TransitionError> {
        if self.phase != expected {
            return Err(TransitionError {
                from: self.phase,
                action,
            });
        }
        apply(self);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(uid: u32) -> Identity {
        Identity::UnixUser { uid }
    }

    fn group(name: &str) -> Identity {
        Identity::Other {
            kind: name.to_owned(),
        }
    }

    fn request() -> AuthRequest {
        AuthRequest::new("cookie-1", "org.example.action", "Install?", user(1000))
    }

    #[test]
    fn identity_prefers_the_agent_user() {
        let offered = [user(0), user(1000)];
        assert_eq!(choose_identity(&offered, 1000), Some(&user(1000)));
    }

    #[test]
    fn identity_falls_back_to_first_user_in_polkit_order() {
        let offered = [group("unix-group"), user(0), user(42)];
        assert_eq!(choose_identity(&offered, 1000), Some(&user(0)));
    }

    #[test]
    fn identity_never_picks_a_group_or_nothing() {
        assert_eq!(choose_identity(&[], 1000), None);
        assert_eq!(choose_identity(&[group("unix-group")], 1000), None);
    }

    #[test]
    fn identity_for_root_agent_prefers_root() {
        let offered = [user(1000), user(0)];
        assert_eq!(choose_identity(&offered, 0), Some(&user(0)));
    }

    #[test]
    fn begin_moves_idle_to_prompting_and_counts_attempts() {
        let mut state = request();
        assert_eq!(state.phase(), Phase::Idle);
        state.begin(false).unwrap();
        assert_eq!(state.phase(), Phase::Prompting);
        assert_eq!(state.attempt(), 1);
        assert!(!state.shows_wrong_password());
    }

    #[test]
    fn retry_after_wrong_password_shows_the_message() {
        let mut state = request();
        state.begin(true).unwrap();
        assert!(state.shows_wrong_password());
        state.submit().unwrap();
        assert!(!state.shows_wrong_password(), "message is for prompting only");
    }

    #[test]
    fn submit_then_verified_answers() {
        let mut state = request();
        state.begin(false).unwrap();
        state.submit().unwrap();
        assert_eq!(state.phase(), Phase::Verifying);
        state.verified().unwrap();
        assert_eq!(state.phase(), Phase::Answered);
    }

    #[test]
    fn rejected_maps_to_wrong_password_or_generic_failure() {
        let mut wrong = request();
        wrong.begin(false).unwrap();
        wrong.submit().unwrap();
        wrong.rejected(true).unwrap();
        assert_eq!(wrong.phase(), Phase::Failed(Failure::WrongPassword));

        let mut other = request();
        other.begin(false).unwrap();
        other.submit().unwrap();
        other.rejected(false).unwrap();
        assert_eq!(other.phase(), Phase::Failed(Failure::Helper));
    }

    #[test]
    fn cancel_is_allowed_before_answer_and_during_verification() {
        for setup in [0, 1, 2] {
            let mut state = request();
            if setup >= 1 {
                state.begin(false).unwrap();
            }
            if setup == 2 {
                state.submit().unwrap();
            }
            state.cancel().unwrap();
            assert_eq!(state.phase(), Phase::Cancelled);
        }
    }

    #[test]
    fn fail_from_idle_prompting_and_verifying() {
        let mut idle = request();
        idle.fail(Failure::NoIdentity, "fail").unwrap();
        assert_eq!(idle.phase(), Phase::Failed(Failure::NoIdentity));

        let mut prompting = request();
        prompting.begin(false).unwrap();
        prompting.fail(Failure::HelperUnavailable, "fail").unwrap();
        assert_eq!(prompting.phase(), Phase::Failed(Failure::HelperUnavailable));

        let mut verifying = request();
        verifying.begin(false).unwrap();
        verifying.submit().unwrap();
        verifying.fail(Failure::Helper, "fail").unwrap();
        assert_eq!(verifying.phase(), Phase::Failed(Failure::Helper));
    }

    #[test]
    fn out_of_order_transitions_are_refused_and_leave_the_phase() {
        let mut state = request();
        let error = state.submit().unwrap_err();
        assert_eq!(error.from, Phase::Idle);
        assert_eq!(state.phase(), Phase::Idle);

        let error = state.verified().unwrap_err();
        assert_eq!(error.from, Phase::Idle);
        assert_eq!(state.phase(), Phase::Idle);

        state.begin(false).unwrap();
        let error = state.begin(false).unwrap_err();
        assert_eq!(error.from, Phase::Prompting);
        assert_eq!(state.attempt(), 1, "a refused begin does not count");
        let error = state.verified().unwrap_err();
        assert_eq!(error.from, Phase::Prompting);
        assert_eq!(state.phase(), Phase::Prompting);
    }

    #[test]
    fn terminal_phases_refuse_every_transition() {
        let terminals = [
            Phase::Answered,
            Phase::Cancelled,
            Phase::Failed(Failure::Helper),
        ];
        for terminal in terminals {
            let mut state = request();
            state.phase = terminal;
            assert!(state.begin(false).is_err());
            assert!(state.submit().is_err());
            assert!(state.verified().is_err());
            assert!(state.rejected(true).is_err());
            assert!(state.fail(Failure::Helper, "fail").is_err());
            assert!(state.cancel().is_err());
            assert_eq!(state.phase(), terminal, "{terminal:?} must stay put");
        }
    }

    #[test]
    fn every_failure_has_a_visible_message() {
        for failure in [
            Failure::NoIdentity,
            Failure::HelperUnavailable,
            Failure::WrongPassword,
            Failure::Helper,
        ] {
            assert!(!failure.message().is_empty());
        }
        assert_eq!(Failure::WrongPassword.message(), "Wrong password");
    }
}
