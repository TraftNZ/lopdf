use std::convert::{TryFrom, TryInto};
use std::io::{Error, ErrorKind, Read, Result, Write};
use std::mem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
    None,
    Sub,
    Up,
    Avg,
    Paeth,
}

impl TryFrom<u8> for FilterType {
    type Error = ();

    fn try_from(n: u8) -> std::result::Result<FilterType, ()> {
        match n {
            0 => Ok(FilterType::None),
            1 => Ok(FilterType::Sub),
            2 => Ok(FilterType::Up),
            3 => Ok(FilterType::Avg),
            4 => Ok(FilterType::Paeth),
            _ => Err(()),
        }
    }
}

fn paeth_predict(left: u8, above: u8, upperleft: u8) -> u8 {
    let expand_left = i16::from(left);
    let expand_above = i16::from(above);
    let expand_upperleft = i16::from(upperleft);

    let initial_estimate = expand_left + expand_above - expand_upperleft;

    let dist_left = (initial_estimate - expand_left).abs();
    let dist_above = (initial_estimate - expand_above).abs();
    let dist_upperleft = (initial_estimate - expand_upperleft).abs();

    if dist_left <= dist_above && dist_left <= dist_upperleft {
        left
    } else if dist_above <= dist_upperleft {
        above
    } else {
        upperleft
    }
}

pub fn decode_row(filter: FilterType, bpp: usize, previous: &[u8], current: &mut [u8]) {
    use self::FilterType::*;
    let len = current.len();
    let bpp = bpp.min(len);

    match filter {
        None => (),
        Sub => {
            for i in bpp..len {
                current[i] = current[i].wrapping_add(current[i - bpp]);
            }
        }
        Up => {
            for i in 0..len {
                current[i] = current[i].wrapping_add(previous[i]);
            }
        }
        Avg => {
            for i in 0..bpp {
                current[i] = current[i].wrapping_add(previous[i] / 2);
            }

            for i in bpp..len {
                current[i] = current[i].wrapping_add(((u16::from(current[i - bpp]) + u16::from(previous[i])) / 2) as u8);
            }
        }
        Paeth => {
            for i in 0..bpp {
                current[i] = current[i].wrapping_add(paeth_predict(0, previous[i], 0));
            }

            for i in bpp..len {
                current[i] = current[i].wrapping_add(paeth_predict(current[i - bpp], previous[i], previous[i - bpp]));
            }
        }
    }
}

pub fn decode_frame(content: &[u8], bytes_per_pixel: usize, pixels_per_row: usize) -> Result<Vec<u8>> {
    let bytes_per_row = bytes_per_pixel * pixels_per_row;
    let mut previous = Vec::new();
    previous.try_reserve(bytes_per_row)?;
    previous.resize(bytes_per_row, 0_u8);
    let mut current = Vec::new();
    current.try_reserve(bytes_per_row)?;
    current.resize(bytes_per_row, 0_u8);
    let mut decoded = Vec::new();
    let mut pos = 0;
    while pos < content.len() {
        if let Ok(filter) = content[pos].try_into() {
            pos += 1;
            (&content[pos..]).read_exact(current.as_mut_slice())?;
            pos += bytes_per_row;

            decode_row(filter, bytes_per_pixel, previous.as_slice(), current.as_mut_slice());
            decoded.write_all(current.as_slice())?;
            mem::swap(&mut previous, &mut current);
        } else {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("invalid PNG filter type ({})", content[pos]),
            ));
        }
    }
    Ok(decoded)
}

pub fn encode_row(method: FilterType, bpp: usize, previous: &[u8], current: &mut [u8]) {
    use self::FilterType::*;
    let len = current.len();
    let bpp = bpp.min(len);

    match method {
        None => (),
        Sub => {
            for i in (bpp..len).rev() {
                current[i] = current[i].wrapping_sub(current[i - bpp]);
            }
        }
        Up => {
            for i in 0..len {
                current[i] = current[i].wrapping_sub(previous[i]);
            }
        }
        Avg => {
            for i in (bpp..len).rev() {
                current[i] = current[i].wrapping_sub(((u16::from(current[i - bpp]) + u16::from(previous[i])) / 2) as u8);
            }

            for i in 0..bpp {
                current[i] = current[i].wrapping_sub(previous[i] / 2);
            }
        }
        Paeth => {
            for i in (bpp..len).rev() {
                current[i] = current[i].wrapping_sub(paeth_predict(current[i - bpp], previous[i], previous[i - bpp]));
            }

            for i in 0..bpp {
                current[i] = current[i].wrapping_sub(paeth_predict(0, previous[i], 0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn average_filter_floors_the_sum_of_left_and_above() {
        let previous = [10_u8, 200, 255, 7, 0, 130];
        let mut current = [1_u8, 2, 3, 4, 5, 6];
        decode_row(FilterType::Avg, 1, &previous, &mut current);
        // Each byte adds floor((left + above) / 2), summed before halving.
        assert_eq!(current, [6, 105, 183, 99, 54, 98]);
    }

    #[test]
    fn average_filter_round_trips() {
        let previous = [250_u8, 3, 255, 128, 0, 77, 200, 1];
        let original = [255_u8, 254, 1, 129, 64, 200, 7, 250];
        for bpp in [1, 2, 3] {
            let mut row = original;
            encode_row(FilterType::Avg, bpp, &previous, &mut row);
            decode_row(FilterType::Avg, bpp, &previous, &mut row);
            assert_eq!(row, original);
        }
    }
}
