//! The machine's own life cycle.

use omega_proto::omega::{Lock, Reboot, Shutdown, Sleep, action};

use crate::context::Context;
use crate::effect::does;

/// Permission to lock, suspend, or stop the machine.
#[derive(Debug)]
pub struct Session {
    context: Context,
}

does!(Session, SystemControl);

impl Session {
    pub fn lock(&self) -> crate::effect::Effect {
        self.act(action::Kind::Lock(Lock {}))
    }

    pub fn sleep(&self) -> crate::effect::Effect {
        self.act(action::Kind::Sleep(Sleep {}))
    }

    pub fn reboot(&self) -> crate::effect::Effect {
        self.act(action::Kind::Reboot(Reboot {}))
    }

    pub fn shut_down(&self) -> crate::effect::Effect {
        self.act(action::Kind::Shutdown(Shutdown {}))
    }
}
