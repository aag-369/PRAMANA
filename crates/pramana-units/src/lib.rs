//! # pramana-units
//!
//! Dimensionally-typed quantities for cryptographic resource estimation.
//!
//! This crate implements **Law 6** of the PRAMANA specification:
//!
//! > Physical qubits, logical qubits, Toffoli count, T count, code cycles, seconds,
//! > qubit-seconds, and qubit-rounds are distinct newtypes. Arithmetic between
//! > incompatible units must not compile.
//!
//! Conflating logical and physical qubits, or code cycles and seconds, is the single
//! most common class of error in published resource estimates. Here it is a compile
//! error rather than a wrong answer.
//!
//! ## Incompatible arithmetic does not compile
//!
//! ```compile_fail
//! use pramana_units::{LogicalQubits, PhysicalQubits};
//! let l = LogicalQubits::new(10);
//! let p = PhysicalQubits::new(10);
//! let _ = l + p; // error[E0308]: mismatched types
//! ```
//!
//! ```compile_fail
//! use pramana_units::{ToffoliCount, TCount};
//! let a = ToffoliCount::new(4);
//! let b = TCount::new(4);
//! let _ = a + b; // error[E0308]: mismatched types
//! ```
//!
//! ```compile_fail
//! use pramana_units::{CodeCycles, Seconds};
//! let c = CodeCycles::new(1000);
//! let s = Seconds::new(1.0);
//! let _ = c + s; // error[E0308]: mismatched types
//! ```
//!
//! ## Compatible arithmetic does compile
//!
//! ```
//! use pramana_units::{LogicalQubits, PhysicalQubits, CodeCycles, QubitRounds};
//! let a = LogicalQubits::new(10) + LogicalQubits::new(5);
//! assert_eq!(a.get(), 15);
//!
//! // Deliberate, meaningful dimensional composition: qubits x rounds = qubit-rounds
//! let v: QubitRounds = PhysicalQubits::new(1000) * CodeCycles::new(2000);
//! assert_eq!(v.get(), 2_000_000);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error_rate;
mod gates;
mod qubits;
mod time;
mod volume;

pub use error_rate::{Distance, ErrorRate, UnitError};
pub use gates::{
    CliffordCount, DecompositionStrategy, MeasurementCount, TCount, ToffoliCount,
};
pub use qubits::{LogicalQubits, PhysicalQubits};
pub use time::{CodeCycles, Seconds};
pub use volume::QubitRounds;

/// Internal macro generating a dimensionally-distinct integer-valued newtype.
///
/// Each generated type supports addition and subtraction with **itself only**, and
/// scaling by a bare integer. It deliberately does not implement any operation that
/// would allow it to mix with a different unit.
#[macro_export]
#[doc(hidden)]
macro_rules! count_newtype {
    ($(#[$meta:meta])* $name:ident, $inner:ty, $unit:literal) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default,
            ::serde::Serialize, ::serde::Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name($inner);

        impl $name {
            #[doc = concat!("The zero value of `", stringify!($name), "`.")]
            pub const ZERO: Self = Self(0);

            #[doc = concat!("Construct a `", stringify!($name), "` from a raw count.")]
            #[inline]
            pub const fn new(v: $inner) -> Self {
                Self(v)
            }

            #[doc = concat!("Extract the raw count. Use sparingly; prefer typed operations.")]
            #[inline]
            pub const fn get(self) -> $inner {
                self.0
            }

            /// Lossy conversion to `f64`, for reporting and curve fitting only.
            #[inline]
            pub fn as_f64(self) -> f64 {
                self.0 as f64
            }

            /// Saturating addition. Resource counts must never silently wrap.
            #[inline]
            pub const fn saturating_add(self, rhs: Self) -> Self {
                Self(self.0.saturating_add(rhs.0))
            }

            /// Saturating multiplication by a bare scalar.
            #[inline]
            pub const fn saturating_scale(self, k: $inner) -> Self {
                Self(self.0.saturating_mul(k))
            }

            /// Checked addition, returning `None` on overflow.
            #[inline]
            pub const fn checked_add(self, rhs: Self) -> Option<Self> {
                match self.0.checked_add(rhs.0) {
                    Some(v) => Some(Self(v)),
                    None => None,
                }
            }

            /// The unit symbol, for display and serialisation of provenance records.
            pub const UNIT: &'static str = $unit;
        }

        impl ::core::ops::Add for $name {
            type Output = Self;
            #[inline]
            fn add(self, rhs: Self) -> Self {
                Self(self.0.checked_add(rhs.0).expect(concat!(
                    stringify!($name), " overflow in addition"
                )))
            }
        }

        impl ::core::ops::AddAssign for $name {
            #[inline]
            fn add_assign(&mut self, rhs: Self) {
                *self = *self + rhs;
            }
        }

        impl ::core::ops::Sub for $name {
            type Output = Self;
            #[inline]
            fn sub(self, rhs: Self) -> Self {
                Self(self.0.checked_sub(rhs.0).expect(concat!(
                    stringify!($name), " underflow in subtraction"
                )))
            }
        }

        impl ::core::ops::Mul<$inner> for $name {
            type Output = Self;
            #[inline]
            fn mul(self, k: $inner) -> Self {
                Self(self.0.checked_mul(k).expect(concat!(
                    stringify!($name), " overflow in scaling"
                )))
            }
        }

        impl ::core::iter::Sum for $name {
            #[inline]
            fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
                iter.fold(Self::ZERO, |a, b| a + b)
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                write!(f, "{} {}", self.0, $unit)
            }
        }
    };
}
