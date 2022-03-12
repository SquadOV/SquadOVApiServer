use nom::{
    IResult,
    number::streaming::{
        le_f32,
    },
};

#[derive(Debug, Clone)]
pub struct CsgoVector {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for CsgoVector {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }
}

#[derive(Debug)]
pub struct CsgoBoundingBox {
    pub min: CsgoVector,
    pub max: CsgoVector,
}

impl Default for CsgoBoundingBox {
    fn default() -> Self {
        Self {
            min: CsgoVector::default(),
            max: CsgoVector::default(),
        }
    }
}

impl CsgoBoundingBox {
    pub fn contains(&self, v: &CsgoVector) -> bool {
        return v.x >= self.min.x && v.x <= self.max.x &&
        v.y >= self.min.y && v.y <= self.max.y &&
        v.z >= self.min.z && v.z <= self.max.z;
    }
}

pub fn parse_csgo_vector(input: &[u8]) -> IResult<&[u8], CsgoVector> {
    let (input, x) = le_f32(input)?;
    let (input, y) = le_f32(input)?;
    let (input, z) = le_f32(input)?;

    Ok((input, CsgoVector{
        x,
        y,
        z,
    }))
}

#[derive(Debug)]
pub struct CsgoQAngle {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub fn parse_csgo_qangle(input: &[u8]) -> IResult<&[u8], CsgoQAngle> {
    let (input, x) = le_f32(input)?;
    let (input, y) = le_f32(input)?;
    let (input, z) = le_f32(input)?;

    Ok((input, CsgoQAngle{
        x,
        y,
        z,
    }))
}