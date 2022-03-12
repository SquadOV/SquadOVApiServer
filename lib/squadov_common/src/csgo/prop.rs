use crate::bit_reader::{
    CsgoCoordType,
    BitReader,
};
use crate::SquadOvError;
use super::data_table::{CsgoServerClassFlatPropEntry};
use super::prop_types::{
    CsgoPropType,
    SPROP_UNSIGNED,
    SPROP_VARINT,
    SPROP_COORD,
    SPROP_COORD_MP,
    SPROP_COORD_MP_LOWPRECISION,
    SPROP_COORD_MP_INTEGRAL,
    SPROP_NOSCALE,
    SPROP_NORMAL,
    SPROP_CELL_COORD,
    SPROP_CELL_COORD_LOWPRECISION,
    SPROP_CELL_COORD_INTEGRAL,
};
use super::math::CsgoVector;
use std::convert::TryFrom;
use crate::proto::csgo::{
    csvc_msg_send_table,
};

const DT_MAX_STRING_BITS: usize = 9;

#[allow(dead_code)]
#[derive(Debug)]
pub struct CsgoPropValue {
    prop_type: CsgoPropType,
    pub v_i32: Option<i32>,
    pub v_float: Option<f32>,
    pub v_str: Option<String>,
    pub v_i64: Option<i64>,
    pub v_vec: Option<CsgoVector>,
    pub v_arr: Vec<CsgoProp>,
}

impl CsgoPropValue {
    fn generate_default(typ: CsgoPropType) -> Self {
        Self {
            prop_type: typ,
            v_i32: None,
            v_float: None,
            v_str: None,
            v_i64: None,
            v_vec: None,
            v_arr: vec![], 
        }
    }

    fn read_int(reader: &mut BitReader, send_prop: &csvc_msg_send_table::SendpropT) -> Result<i32, SquadOvError> {
        let flags = send_prop.flags();
        Ok(if flags & SPROP_VARINT > 0 {
            if flags & SPROP_UNSIGNED > 0 {
                reader.read_var_u32()? as i32
            } else {
                reader.read_var_i32()?
            }
        } else {
            let num_bits = send_prop.num_bits() as usize;
            if flags & SPROP_UNSIGNED > 0 {
                reader.read_multibit::<u32>(num_bits)? as i32
            } else {
                reader.read_signed_multibit(num_bits)?
            }
        })
    }

    fn parse_int(reader: &mut BitReader, send_prop: &csvc_msg_send_table::SendpropT) -> Result<Self, SquadOvError> {
        let mut value = Self::generate_default(CsgoPropType::DptInt);
        value.v_i32 = Some(Self::read_int(reader, send_prop)?);
        Ok(value)
    }

    fn read_float(reader: &mut BitReader, send_prop: &csvc_msg_send_table::SendpropT) -> Result<f32, SquadOvError> {
        let num_bits = send_prop.num_bits() as usize;
        let flag = send_prop.flags();
        Ok(if flag & SPROP_COORD > 0 {
            reader.read_csgo_bit_coord()?
        } else if flag & SPROP_COORD_MP > 0 {
            reader.read_csgo_bit_coord_mp(CsgoCoordType::None)?
        } else if flag & SPROP_COORD_MP_LOWPRECISION > 0 {
            reader.read_csgo_bit_coord_mp(CsgoCoordType::LowPrecision)?
        } else if flag & SPROP_COORD_MP_INTEGRAL > 0 {
            reader.read_csgo_bit_coord_mp(CsgoCoordType::Integral)?
        } else if flag & SPROP_NOSCALE > 0 {
            reader.read_f32()?
        } else if flag & SPROP_NORMAL > 0 {
            reader.read_csgo_bit_normal()?
        } else if flag & SPROP_CELL_COORD > 0 {
            reader.read_csgo_bit_cell_coord(num_bits, CsgoCoordType::None)?
        } else if flag & SPROP_CELL_COORD_LOWPRECISION > 0 {
            reader.read_csgo_bit_cell_coord(num_bits, CsgoCoordType::LowPrecision)?
        } else if flag & SPROP_CELL_COORD_INTEGRAL > 0 {
            reader.read_csgo_bit_cell_coord(num_bits, CsgoCoordType::Integral)?
        } else {
            let interp = reader.read_multibit::<u32>(num_bits)? as f32 / ((1 << num_bits) - 1) as f32;
            let low = send_prop.low_value();
            let high = send_prop.high_value();
            low + (high - low) * interp

        })
    }

    fn parse_float(reader: &mut BitReader, send_prop: &csvc_msg_send_table::SendpropT) -> Result<Self, SquadOvError> {
        let mut value = Self::generate_default(CsgoPropType::DptFloat);
        value.v_float = Some(Self::read_float(reader, send_prop)?);
        Ok(value)
    }

    fn read_vector(reader: &mut BitReader, send_prop: &csvc_msg_send_table::SendpropT) -> Result<CsgoVector, SquadOvError> {
        let flag = send_prop.flags();
        let x = Self::read_float(reader, send_prop)?;
        let y = Self::read_float(reader, send_prop)?;
        Ok(CsgoVector{
            x,
            y,
            z: if flag & SPROP_NORMAL > 0 {
                let sign = reader.read_bit()?;
                let xy_sqr = x * x + y * y;
                let val = if xy_sqr < 1.0 {
                    (1.0 - xy_sqr).sqrt()
                } else {
                    0.0
                };

                if sign {
                    -val
                } else {
                    val
                }
            } else {
                Self::read_float(reader, send_prop)?
            },
        })
    }

    fn parse_vector(reader: &mut BitReader, send_prop: &csvc_msg_send_table::SendpropT) -> Result<Self, SquadOvError> {
        let mut value = Self::generate_default(CsgoPropType::DptVector);
        value.v_vec = Some(Self::read_vector(reader, send_prop)?);
        Ok(value)
    }

    fn read_vector_xy(reader: &mut BitReader, send_prop: &csvc_msg_send_table::SendpropT) -> Result<CsgoVector, SquadOvError> {
        Ok(CsgoVector{
            x: Self::read_float(reader, send_prop)?,
            y: Self::read_float(reader, send_prop)?,
            z: 0.0,
        })
    }

    fn parse_vector_xy(reader: &mut BitReader, send_prop: &csvc_msg_send_table::SendpropT) -> Result<Self, SquadOvError> {
        let mut value = Self::generate_default(CsgoPropType::DptVectorXy);
        value.v_vec = Some(Self::read_vector_xy(reader, send_prop)?);
        Ok(value)
    }

    fn read_string(reader: &mut BitReader) -> Result<String, SquadOvError> {
        let len = reader.read_multibit::<usize>(DT_MAX_STRING_BITS)?;
        let raw = reader.read_raw(len * 8)?;
        Ok(String::from_utf8(raw)?)
    }

    fn parse_string(reader: &mut BitReader) -> Result<Self, SquadOvError> {
        let mut value = Self::generate_default(CsgoPropType::DptString);
        value.v_str = Some(Self::read_string(reader)?);
        Ok(value)
    }

    fn read_array(reader: &mut BitReader, prop: &CsgoServerClassFlatPropEntry, max_elements: i32) -> Result<Vec<CsgoProp>, SquadOvError> {
        let num_bits = crate::math::integer_log2(max_elements) as usize + 1;
        let num_elements = reader.read_multibit::<usize>(num_bits)?;
        let mut ret: Vec<CsgoProp> = vec![];
        ret.reserve(num_elements);
        
        for i in 0..num_elements {    
            let element_prop = CsgoServerClassFlatPropEntry{
                prop: prop.array_element_prop.clone().ok_or(SquadOvError::BadRequest)?,
                array_element_prop: None,
                prefix: format!("{}.{:03}", &prop.prefix, i),
            };
            ret.push(CsgoProp::parse(reader, &element_prop)?);
        }

        Ok(ret)
    }

    fn parse_array(reader: &mut BitReader, prop: &CsgoServerClassFlatPropEntry, max_elements: i32) -> Result<Self, SquadOvError> {
        let mut value = Self::generate_default(CsgoPropType::DptArray);
        value.v_arr = Self::read_array(reader, prop, max_elements)?;
        Ok(value)
    }

    fn read_i64(reader: &mut BitReader, send_prop: &csvc_msg_send_table::SendpropT) -> Result<i64, SquadOvError> {
        let flags = send_prop.flags();
        let num_bits = send_prop.num_bits() as usize;
        Ok(if flags & SPROP_VARINT > 0 {
            if flags & SPROP_UNSIGNED > 0 {
                reader.read_var_u64()? as i64
            } else {
                reader.read_var_i64()?
            }
        } else {
            let high_int: u32;
            let low_int: u32;
            let neg: bool;
            if flags & SPROP_UNSIGNED > 0 {
                neg = false;
                low_int = reader.read_multibit::<u32>(32)?;
                high_int = reader.read_multibit::<u32>(num_bits - 32 - 1)?;
            } else {
                neg = reader.read_bit()?;
                low_int = reader.read_multibit::<u32>(32)?;
                high_int = reader.read_multibit::<u32>(num_bits - 32)?;
            }

            let ret: i64 = low_int as i64 + ((high_int as i64) << 32);
            if neg {
                -ret
            } else {
                ret
            }
        })
    }

    fn parse_i64(reader: &mut BitReader, send_prop: &csvc_msg_send_table::SendpropT) -> Result<Self, SquadOvError> {
        let mut value = Self::generate_default(CsgoPropType::DptArray);
        value.v_i64 = Some(Self::read_i64(reader, send_prop)?);
        Ok(value)
    }
}

#[derive(Debug)]
pub struct CsgoProp {
    entry: CsgoServerClassFlatPropEntry,
    pub value: CsgoPropValue,
}

impl CsgoProp {
    pub fn parse(reader: &mut BitReader, prop_entry: &CsgoServerClassFlatPropEntry) -> Result<Self, SquadOvError> {
        if let Some(send_prop) = prop_entry.prop.get() {
            let prop_type = CsgoPropType::try_from(send_prop.r#type())?;
            Ok(Self {
                entry: prop_entry.clone(),
                value: match prop_type {
                    CsgoPropType::DptInt => CsgoPropValue::parse_int(reader, send_prop)?,
                    CsgoPropType::DptFloat => CsgoPropValue::parse_float(reader, send_prop)?,
                    CsgoPropType::DptVector => CsgoPropValue::parse_vector(reader, send_prop)?,
                    CsgoPropType::DptVectorXy => CsgoPropValue::parse_vector_xy(reader, send_prop)?,
                    CsgoPropType::DptString => CsgoPropValue::parse_string(reader)?,
                    CsgoPropType::DptArray => CsgoPropValue::parse_array(reader, prop_entry, send_prop.num_elements())?,
                    CsgoPropType::DptInt64 => CsgoPropValue::parse_i64(reader, send_prop)?,
                    _ => {
                        log::warn!("Unsupported prop type: {:?}", prop_type);
                        return Err(SquadOvError::BadRequest);
                    },
                }
            })
        } else {
            log::warn!("Parsing a prop entry that doesn't exist?");
            Err(SquadOvError::NotFound)
        }
    }

    pub fn entry(&self) -> &CsgoServerClassFlatPropEntry {
        &self.entry
    }
}