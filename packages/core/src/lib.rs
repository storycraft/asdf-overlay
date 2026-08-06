pub mod input;
pub mod overlay;
pub mod surface;

use asdf_overlay_client::common::size;
use mimalloc::MiMalloc;
use napi_derive::napi;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[napi(discriminant_case = "snake_case")]
pub enum PercentLength {
    Percent { value: f64 },
    Length { value: f64 },
}

impl From<PercentLength> for size::PercentLength {
    fn from(val: PercentLength) -> Self {
        match val {
            PercentLength::Percent { value } => size::PercentLength::Percent(value as _),
            PercentLength::Length { value } => size::PercentLength::Length(value as _),
        }
    }
}
