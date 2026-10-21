use std::fmt;

use super::interne::Limite;

/// Un servomoteur.
#[derive(Debug, Clone)]
pub struct Servo {
    angle: u32,
    limite: Limite,
}

impl Servo {
    pub fn new(angle: u32) -> Self {
        Self { angle, limite: Limite { max: 180 } }
    }

    pub fn tourner(&mut self, delta: u32) {
        self.angle = (self.angle + delta).min(self.limite.max);
    }

    pub fn angle(&self) -> u32 {
        self.angle
    }
}

impl fmt::Display for Servo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "servo {}°", self.angle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tourne() {
        let mut s = Servo::new(1);
        s.tourner(1);
        assert_eq!(s.angle(), 2);
    }
}
