//! The machine's own life cycle.

use omega_wire::omega::{Lock, Reboot, Shutdown, Sleep, action};

use crate::context::Context;
use crate::effect::does;

/// Permission to lock, suspend, or stop the machine.
#[derive(Debug)]
pub struct Session {
    context: Context,
}

does!(Session, SystemControl);

impl Session {
    pub fn lock(&self) {
        self.act(action::Kind::Lock(Lock {}));
    }

    pub fn sleep(&self) {
        self.act(action::Kind::Sleep(Sleep {}));
    }

    pub fn reboot(&self) {
        self.act(action::Kind::Reboot(Reboot {}));
    }

    pub fn shut_down(&self) {
        self.act(action::Kind::Shutdown(Shutdown {}));
    }
}
