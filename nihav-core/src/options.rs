//! Options support.
//!
//! This module contains the definitions for options and names for common options.
//! Options are used to set custom parameters in e.g. decoders or muxers.
//!
//! As a rule target for options should provide a list of supported options and ignore unknown options.

use std::sync::Arc;

/// A list specifying option parsing and validating errors.
#[derive(Clone,Copy,Debug,PartialEq)]
pub enum OptionError {
    /// Input is not intended for the current option definition.
    WrongName,
    /// Option value is not in the expected format.
    InvalidFormat,
    /// Option value was not in the range.
    InvalidValue,
    /// Parse error.
    ParseError,
}

/// A specialised `Result` type for option parsing/validation.
pub type OptionResult<T> = Result<T, OptionError>;

/// Option definition.
#[derive(Debug)]
pub struct NAOptionDefinition {
    /// Option name.
    pub name:           &'static str,
    /// Option meaning.
    pub description:    &'static str,
    /// Minimal value for the option (if applicable).
    pub min_value:      Option<NAValue>,
    /// Maximum value for the option (if applicable).
    pub max_value:      Option<NAValue>,
    /// Allowed string values (when value is a string).
    pub valid_strings:  Option<Vec<String>>,
    /// Default option value.
    ///
    /// This is used mainly to tell in which format options should be (e.g. bool or float).
    pub default_value:  NAValue,
}

impl NAOptionDefinition {
    /// Tries to parse input string(s) as an option and returns new option and number of arguments used (1 or 2) on success.
    pub fn parse(&self, name: &String, value: Option<&String>) -> OptionResult<(NAOption, usize)> {
        let no_name = "no".to_owned() + self.name;
        let opt_no_name = "--no".to_owned() + self.name;
        if name == &no_name || name == &opt_no_name {
            match self.default_value {
                NAValue::Bool(_) => return Ok((NAOption { name: self.name, value: NAValue::Bool(false) }, 1)),
                _ => return Err(OptionError::InvalidFormat),
            };
        }
        let opt_name = "--".to_owned() + self.name;
        if self.name != name && &opt_name != name {
            return Err(OptionError::WrongName);
        }
        match self.default_value {
            NAValue::None => Ok((NAOption { name: self.name, value: NAValue::None }, 1)),
            NAValue::Bool(_) => Ok((NAOption { name: self.name, value: NAValue::Bool(true) }, 1)),
            NAValue::Int(_) => {
                if let Some(str) = value {
                    let ret = str.parse::<i32>();
                    if let Ok(val) = ret {
                        let opt = NAOption { name: self.name, value: NAValue::Int(val) };
                        self.check(&opt)?;
                        Ok((opt, 2))
                    } else {
                        Err(OptionError::ParseError)
                    }
                } else {
                    Err(OptionError::ParseError)
                }
            },
            NAValue::Long(_) => {
                if let Some(str) = value {
                    let ret = str.parse::<i64>();
                    if let Ok(val) = ret {
                        let opt = NAOption { name: self.name, value: NAValue::Long(val) };
                        self.check(&opt)?;
                        Ok((opt, 2))
                    } else {
                        Err(OptionError::ParseError)
                    }
                } else {
                    Err(OptionError::ParseError)
                }
            },
            NAValue::Float(_) => {
                if let Some(str) = value {
                    let ret = str.parse::<f64>();
                    if let Ok(val) = ret {
                        let opt = NAOption { name: self.name, value: NAValue::Float(val) };
                        self.check(&opt)?;
                        Ok((opt, 2))
                    } else {
                        Err(OptionError::ParseError)
                    }
                } else {
                    Err(OptionError::ParseError)
                }
            },
            NAValue::String(_) => {
                if let Some(str) = value {
                    let opt = NAOption { name: self.name, value: NAValue::String(str.to_string()) };
                    self.check(&opt)?;
                    Ok((opt, 2))
                } else {
                    Err(OptionError::ParseError)
                }
            },
            _ => unimplemented!(),
        }
    }
    /// Checks whether input option conforms to the definition i.e. whether it has proper format and it lies in range.
    pub fn check(&self, option: &NAOption) -> OptionResult<()> {
        if option.name != self.name {
            return Err(OptionError::WrongName);
        }
        match option.value {
            NAValue::None => Ok(()),
            NAValue::Bool(_) => Ok(()),
            NAValue::Int(intval) => {
                if let Some(ref minval) = self.min_value {
                    let (minres, _) = Self::compare(i64::from(intval), minval)?;
                    if !minres {
                        return Err(OptionError::InvalidValue);
                    }
                }
                if let Some(ref maxval) = self.max_value {
                    let (_, maxres) = Self::compare(i64::from(intval), maxval)?;
                    if !maxres {
                        return Err(OptionError::InvalidValue);
                    }
                }
                Ok(())
            },
            NAValue::Long(intval) => {
                if let Some(ref minval) = self.min_value {
                    let (minres, _) = Self::compare(intval, minval)?;
                    if !minres {
                        return Err(OptionError::InvalidValue);
                    }
                }
                if let Some(ref maxval) = self.max_value {
                    let (_, maxres) = Self::compare(intval, maxval)?;
                    if !maxres {
                        return Err(OptionError::InvalidValue);
                    }
                }
                Ok(())
            },
            NAValue::Float(fval) => {
                if let Some(ref minval) = self.min_value {
                    let (minres, _) = Self::compare_f64(fval, minval)?;
                    if !minres {
                        return Err(OptionError::InvalidValue);
                    }
                }
                if let Some(ref maxval) = self.max_value {
                    let (_, maxres) = Self::compare_f64(fval, maxval)?;
                    if !maxres {
                        return Err(OptionError::InvalidValue);
                    }
                }
                Ok(())
            },
            NAValue::String(ref cur_str) => {
                if let Some(ref strings) = self.valid_strings {
                    for str in strings.iter() {
                        if cur_str == str {
                            return Ok(());
                        }
                    }
                    Err(OptionError::InvalidValue)
                } else {
                    Ok(())
                }
            },
            NAValue::Data(_) => Ok(()),
        }
    }
    fn compare(val: i64, ref_val: &NAValue) -> OptionResult<(bool, bool)> {
        match ref_val {
            NAValue::Int(ref_min) => {
                Ok((val >= i64::from(*ref_min), val <= i64::from(*ref_min)))
            },
            NAValue::Long(ref_min) => {
                Ok((val >= *ref_min, val <= *ref_min))
            },
            NAValue::Float(ref_min) => {
                Ok(((val as f64) >= *ref_min, (val as f64) <= *ref_min))
            },
            _ => Err(OptionError::InvalidFormat),
        }
    }
    fn compare_f64(val: f64, ref_val: &NAValue) -> OptionResult<(bool, bool)> {
        match ref_val {
            NAValue::Int(ref_min) => {
                Ok((val >= f64::from(*ref_min), val <= f64::from(*ref_min)))
            },
            NAValue::Long(ref_min) => {
                Ok((val >= (*ref_min as f64), val <= (*ref_min as f64)))
            },
            NAValue::Float(ref_min) => {
                Ok((val >= *ref_min, val <= *ref_min))
            },
            _ => Err(OptionError::InvalidFormat),
        }
    }
}

/// Option.
#[derive(Clone,Debug,PartialEq)]
pub struct NAOption {
    /// Option name.
    pub name:   &'static str,
    /// Option value.
    pub value:  NAValue,
}

/// A list of accepted option values.
#[derive(Debug,Clone,PartialEq)]
pub enum NAValue {
    /// Empty value.
    None,
    /// Boolean value.
    Bool(bool),
    /// Integer value.
    Int(i32),
    /// Long integer value.
    Long(i64),
    /// Floating point value.
    Float(f64),
    /// String value.
    String(String),
    /// Binary data value.
    Data(Arc<Vec<u8>>),
}

/// Trait for all objects that handle `NAOption`.
pub trait NAOptionHandler {
    /// Returns the options recognised by current object.
    fn get_supported_options(&self) -> &[NAOptionDefinition];
    /// Passes options for the object to set (or ignore).
    fn set_options(&mut self, options: &[NAOption]);
    /// Queries the current option value in the object (if present).
    fn query_option_value(&self, name: &str) -> Option<NAValue>;
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_option_validation() {
        let option = NAOption {name: "option", value: NAValue::Int(42) };
        let mut def = NAOptionDefinition { name: "option", description: "", min_value: None, max_value: None, valid_strings: None, default_value: NAValue::Float(0.0) };
        assert!(def.check(&option).is_ok());
        def.max_value = Some(NAValue::Long(30));
        assert_eq!(def.check(&option), Err(OptionError::InvalidValue));
        def.max_value = None;
        def.min_value = Some(NAValue::Int(40));
        assert!(def.check(&option).is_ok());
        def.name = "option2";
        assert_eq!(def.check(&option), Err(OptionError::WrongName));
        let option = NAOption {name: "option", value: NAValue::String("test".to_string()) };
        let mut def = NAOptionDefinition { name: "option", description: "", min_value: None, max_value: None, valid_strings: None, default_value: NAValue::String("".to_string()) };
        assert!(def.check(&option).is_ok());
        def.valid_strings = Some(vec!["a string".to_string(), "test string".to_string()]);
        assert_eq!(def.check(&option), Err(OptionError::InvalidValue));
        def.valid_strings = Some(vec!["a string".to_string(), "test".to_string()]);
        assert!(def.check(&option).is_ok());
    }
    #[test]
    fn test_option_parsing() {
        let mut def = NAOptionDefinition { name: "option", description: "", min_value: None, max_value: None, valid_strings: None, default_value: NAValue::Float(0.0) };
        assert_eq!(def.parse(&"--option".to_string(), None), Err(OptionError::ParseError));
        assert_eq!(def.parse(&"--nooption".to_string(), None), Err(OptionError::InvalidFormat));
        assert_eq!(def.parse(&"--option".to_string(), Some(&"42".to_string())),
                   Ok((NAOption{name:"option",value: NAValue::Float(42.0)}, 2)));
        def.max_value = Some(NAValue::Float(40.0));
        assert_eq!(def.parse(&"--option".to_string(), Some(&"42".to_string())),
                   Err(OptionError::InvalidValue));
        let def = NAOptionDefinition { name: "option", description: "", min_value: None, max_value: None, valid_strings: None, default_value: NAValue::Bool(true) };
        assert_eq!(def.parse(&"option".to_string(), None),
                   Ok((NAOption{name: "option", value: NAValue::Bool(true) }, 1)));
        assert_eq!(def.parse(&"nooption".to_string(), None),
                   Ok((NAOption{name: "option", value: NAValue::Bool(false) }, 1)));
    }
}
