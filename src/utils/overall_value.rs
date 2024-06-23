//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::str::FromStr;

use hill_vacuum_shared::return_if_no_match;

use super::misc::ReplaceValues;

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait for types representing the overall value of a list of elements.
pub trait OverallValueInterface<T>
{
    /// Whever `self` now represents a non uniform value.
    #[must_use]
    fn is_not_uniform(&self) -> bool;

    /// Update `self` with `value`.
    /// Returns true if `self` now represents a non uniform value.
    #[must_use]
    fn stack(&mut self, value: &T) -> bool;

    /// Update `self` with another instance of `Self`.
    /// Returns true if `self` now represents a non uniform value.
    #[must_use]
    fn merge(&mut self, other: Self) -> bool;
}

//=======================================================================//

/// A trait to create a textual representation of an overall value.
pub trait OverallValueToUi<T, V>
where
    Self: OverallValueInterface<T> + Sized,
    V: From<Self>
{
    /// Returns a UI-friendly representation of `self`.
    #[must_use]
    fn ui(self) -> V { self.into() }
}

impl<T, U: OverallValueInterface<T>, V: From<U>> OverallValueToUi<T, V> for U {}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The overall value of elements in a list.
#[must_use]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum OverallValue<T>
where
    T: PartialEq + Clone
{
    /// No elements.
    #[default]
    None,
    /// At least one element has value different from the rest.
    NonUniform,
    /// All elements have the same value.
    Uniform(T)
}

impl<T> From<T> for OverallValue<T>
where
    T: PartialEq + Clone
{
    #[inline]
    fn from(value: T) -> Self { Self::new(value) }
}

impl<T> OverallValueInterface<T> for OverallValue<T>
where
    T: PartialEq + Clone
{
    #[inline]
    fn is_not_uniform(&self) -> bool { matches!(self, Self::NonUniform) }

    #[inline]
    fn stack(&mut self, value: &T) -> bool
    {
        match self
        {
            Self::None => *self = Self::Uniform(value.clone()),
            Self::NonUniform => (),
            Self::Uniform(v) =>
            {
                if *v != *value
                {
                    *self = Self::NonUniform;
                }
            }
        };

        self.is_not_uniform()
    }

    #[inline]
    fn merge(&mut self, other: Self) -> bool
    {
        if let Self::None = self
        {
            *self = other;
            return self.is_not_uniform();
        }

        match (&mut *self, other)
        {
            (_, Self::None) | (Self::NonUniform, _) => (),
            (Self::Uniform(_), Self::NonUniform) => *self = Self::NonUniform,
            (Self::Uniform(_), Self::Uniform(v)) => _ = self.stack(&v),
            (Self::None, _) => unreachable!()
        };

        self.is_not_uniform()
    }
}

impl<T: PartialEq + Clone> OverallValue<T>
{
    /// Returns a uniform [`OverallValue`] with `value`.
    #[inline]
    pub const fn new(value: T) -> Self { Self::Uniform(value) }

    /// Whever `self` described at least one value.
    #[inline]
    #[must_use]
    pub const fn is_some(&self) -> bool { !matches!(self, Self::None) }
}

//=======================================================================//

#[must_use]
#[derive(Clone, Debug)]
/// An UI friendly representation of an [`OverallValue`].
enum UiValueEnum<T: ToString + FromStr>
{
    /// No elements.
    None(String),
    /// At least one element has value different from the rest.
    NonUniform(String),
    /// All elements have the same value.
    Uniform
    {
        /// The overall value.
        value:     T,
        /// A [`String`] representation of `value`.
        value_str: String,
        /// The buffer where user types the new desired value.
        buffer:    String
    }
}

impl<T: ToString + FromStr> Default for UiValueEnum<T>
{
    #[inline]
    fn default() -> Self { Self::None(String::new()) }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// A UI-friendly representation of an overall value, useful to show information for selected
/// elements.
#[must_use]
#[derive(Clone, Debug, Default)]
pub struct UiOverallValue<T: ToString + FromStr + PartialEq>(UiValueEnum<T>);

impl<T: ToString + FromStr + PartialEq + Clone> From<OverallValue<T>> for UiOverallValue<T>
{
    #[inline]
    fn from(value: OverallValue<T>) -> Self
    {
        match value
        {
            OverallValue::None => Self::none(),
            OverallValue::NonUniform => Self(UiValueEnum::NonUniform(String::new())),
            OverallValue::Uniform(v) => v.into()
        }
    }
}

impl<T: ToString + FromStr + PartialEq> From<T> for UiOverallValue<T>
{
    #[inline]
    fn from(value: T) -> Self
    {
        let string = value.to_string();

        Self(UiValueEnum::Uniform {
            value,
            value_str: string.clone(),
            buffer: string
        })
    }
}

impl<T: ToString + FromStr + PartialEq> UiOverallValue<T>
{
    /// Returns a [`UiOverallValue`] that represents an empty [`OverallValue`].
    #[inline]
    pub const fn none() -> Self { Self(UiValueEnum::None(String::new())) }

    /// Returns a [`UiOverallValue`] that represents a non uniform [`OverallValue`].
    #[inline]
    pub const fn non_uniform() -> Self { Self(UiValueEnum::NonUniform(String::new())) }

    /// Returns true if [`UiOverallValue`] represents an empty [`OverallValue`].
    #[inline]
    #[must_use]
    pub const fn is_none(&self) -> bool { matches!(self.0, UiValueEnum::None(_)) }

    /// Returns the overall value, if [`UiOverallValue`] represents an uniform value.
    #[inline]
    #[must_use]
    pub fn uniform_value(&self) -> Option<&T>
    {
        return_if_no_match!(&self.0, UiValueEnum::Uniform { value, .. }, value, None).into()
    }

    /// Returns a reference to the [`String`] where the new desired value is being typed.
    #[inline]
    #[must_use]
    const fn buffer(&self) -> &String
    {
        let (UiValueEnum::None(buffer) |
        UiValueEnum::NonUniform(buffer) |
        UiValueEnum::Uniform { buffer, .. }) = &self.0;
        buffer
    }

    /// Returns a mutable reference to the [`String`] where the new desired value is being typed.
    #[inline]
    #[must_use]
    pub fn buffer_mut(&mut self) -> &mut String
    {
        let (UiValueEnum::None(buffer) |
        UiValueEnum::NonUniform(buffer) |
        UiValueEnum::Uniform { buffer, .. }) = &mut self.0;
        buffer
    }

    /// Updates the value with what the user has typed if it can be properly parsed, executing `f`
    /// if it's the case. Otherwise the shown value is reset to what it originally was.
    #[inline]
    pub fn update<F: FnMut(T) -> Option<T>>(
        &mut self,
        gained_focus: bool,
        lost_focus: bool,
        mut f: F
    ) -> bool
    {
        match &mut self.0
        {
            UiValueEnum::Uniform {
                value,
                value_str,
                buffer
            } =>
            {
                if gained_focus
                {
                    self.reset_buffer();
                    return false;
                }

                if !lost_focus
                {
                    return false;
                }

                if let Ok(new_value) = buffer.parse::<T>()
                {
                    let new_value = match f(new_value)
                    {
                        Some(v) =>
                        {
                            if v == *value
                            {
                                return false;
                            }

                            v
                        },
                        None =>
                        {
                            self.reset_buffer();
                            return false;
                        }
                    };

                    *buffer = new_value.to_string();
                    value_str.replace_values(buffer.chars());
                    return true;
                }

                self.reset_buffer();
            },
            UiValueEnum::NonUniform(_) | UiValueEnum::None(_) =>
            {
                if !lost_focus
                {
                    return false;
                }

                if let Ok(Some(value)) = self.buffer().parse::<T>().map(f)
                {
                    let str = value.to_string();

                    self.0 = UiValueEnum::Uniform {
                        value,
                        value_str: str.clone(),
                        buffer: str
                    };
                    return true;
                }

                self.reset_buffer();
            }
        };

        false
    }

    /// Resets the typable buffer to the original value.
    #[inline]
    fn reset_buffer(&mut self)
    {
        match &mut self.0
        {
            UiValueEnum::None(b) | UiValueEnum::NonUniform(b) => b.clear(),
            UiValueEnum::Uniform {
                value_str, buffer, ..
            } => buffer.replace_values(value_str.chars())
        };
    }
}
