//=======================================================================//
// IMPORTS
//
//=======================================================================//

use super::nodes::Movement;
use crate::utils::overall_value::{OverallValue, OverallValueInterface, UiOverallValue};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The overall movement properties of the selected [`Node`]s.
#[must_use]
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::map) struct OverallMovement
{
    /// The overall maximum speed.
    pub max_speed:               OverallValue<f32>,
    /// The overall minimum speed.
    pub min_speed:               OverallValue<f32>,
    /// The overall acceleration.
    pub accel_travel_percentage: OverallValue<f32>,
    /// The overall deceleration.
    pub decel_travel_percentage: OverallValue<f32>,
    /// The overall standby time.
    pub standby_time:            OverallValue<f32>
}

impl From<&Movement> for OverallMovement
{
    #[inline]
    fn from(value: &Movement) -> Self { Self::from_movement(value) }
}

impl OverallValueInterface<Movement> for OverallMovement
{
    #[inline]
    fn stack(&mut self, movement: &Movement) -> bool { self.merge(Self::from(movement)) }

    #[inline]
    fn merge(&mut self, other: Self) -> bool
    {
        let mut uniform = false;

        for (v_0, v_1) in [
            (&mut self.max_speed, other.max_speed),
            (&mut self.min_speed, other.min_speed),
            (&mut self.accel_travel_percentage, other.accel_travel_percentage),
            (&mut self.decel_travel_percentage, other.decel_travel_percentage),
            (&mut self.standby_time, other.standby_time)
        ]
        {
            uniform |= !v_0.merge(v_1);
        }

        !uniform
    }

    #[inline]
    fn is_not_uniform(&self) -> bool
    {
        self.max_speed.is_not_uniform() &&
            self.min_speed.is_not_uniform() &&
            self.accel_travel_percentage.is_not_uniform() &&
            self.decel_travel_percentage.is_not_uniform() &&
            self.standby_time.is_not_uniform()
    }
}

impl OverallMovement
{
    /// Creates a new [`OverallMovement`].
    #[inline]
    pub fn new() -> Self { Self::default() }

    /// Creates a new [`OverallMovement`] with fields initialized with the values from `movement`.
    #[inline]
    pub fn from_movement(movement: &Movement) -> Self
    {
        Self {
            max_speed:               movement.max_speed().into(),
            min_speed:               movement.min_speed().into(),
            accel_travel_percentage: (movement.accel_travel_percentage()).round().into(),
            decel_travel_percentage: (movement.decel_travel_percentage()).round().into(),
            standby_time:            movement.standby_time().into()
        }
    }

    /// Whever `self` was fed any values.
    #[inline]
    #[must_use]
    pub const fn is_some(&self) -> bool { self.max_speed.is_some() }
}

//=======================================================================//

/// A UI friendly representation of [`UiOverallMovement`].
#[must_use]
#[derive(Clone, Debug, Default)]
pub(in crate::map) struct UiOverallMovement
{
    /// The overall maximum speed.
    pub max_speed:               UiOverallValue<f32>,
    /// The overall minimum speed.
    pub min_speed:               UiOverallValue<f32>,
    /// The overall acceleration.
    pub accel_travel_percentage: UiOverallValue<f32>,
    /// The overall deceleration.
    pub decel_travel_percentage: UiOverallValue<f32>,
    /// The overall standby time.
    pub standby_time:            UiOverallValue<f32>
}

impl From<OverallMovement> for UiOverallMovement
{
    #[inline]
    fn from(value: OverallMovement) -> Self
    {
        Self {
            max_speed:               value.max_speed.into(),
            accel_travel_percentage: value.accel_travel_percentage.into(),
            decel_travel_percentage: value.decel_travel_percentage.into(),
            min_speed:               value.min_speed.into(),
            standby_time:            value.standby_time.into()
        }
    }
}
