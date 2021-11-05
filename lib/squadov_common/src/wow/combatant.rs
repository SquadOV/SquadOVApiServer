use serde::Deserialize;
use crate::SquadOvError;
use std::num::Wrapping;

#[derive(Deserialize, Clone)]
pub struct WoWCombatantInfo {
    pub guid: String
}

const A1: i64 = 0x65d200ce55b19ad8;
const B1: i64 = 0x4f2162926e40c299;
const C1: i64 = 0x162dd799029970f8;

pub fn generate_combatants_key(combatants: &[WoWCombatantInfo]) -> String {
    let mut sorted_combatants = combatants.to_vec();
    sorted_combatants.sort_by(|a, b| {
        if a.guid < b.guid {
            return std::cmp::Ordering::Less;
        } else if a.guid > b.guid {
            return std::cmp::Ordering::Greater;
        } else {
            return std::cmp::Ordering::Equal;
        }
    });

    let combined_guids: Vec<String> = sorted_combatants.iter().map(|x| { x.guid.clone() }).collect();
    combined_guids.join("::")
}

// This is needed to determine which players are in the same match using PostgreSQL functionality even in the case
// where not every player has a full view on who's in the match. Are we losing data by converting the GUID to an i32?
// Yes. The hope is that the hashing is good enough that the chances of finding a collision with the small number of
// players in a single match isn't enough to actually find collisions.
pub fn generate_combatants_hashed_array(combatants: &[WoWCombatantInfo]) -> Result<Vec<i32>, SquadOvError> {
    combatants.iter().map(|x| {
        // Player GUID looks like so:
        // Player-SERVER ID-PLAYER ID
        // Where Server ID and Player ID are presumably both hexadecimal.
        // It's probably safe to assume that Player ID is 8 digits long always aka a 32 bit integer.
        // Server ID is variable length but I've seen them with up to 4 digits - aka a 16 bit integer.
        // So we can ultimately create a 48-bit integer (in a 64-bit integer).
        let parts: Vec<&str> = x.guid.split('-').collect();
        
        let server_id = i64::from_str_radix(parts[1], 16)?;
        let player_id =  i64::from_str_radix(parts[2], 16)?;

        let joint_id = (server_id << 32) | player_id;

        // Instead of treating this as a full 64 bit integer (with 32 bit hi and lo parts), we'll treat it
        // as having 24 bit hi and lo parts so we actually hash the hi part with a significant number of bits.
        // Taking this code from here:
        // https://lemire.me/blog/2018/08/15/fast-strongly-universal-64-bit-hashing-everywhere/
        let low = Wrapping(joint_id & 0x0000000000FFFFFF);
        let high = Wrapping((joint_id & 0x0000FFFFFF000000) >> 24);
        Ok(((Wrapping(A1) * low + Wrapping(B1) * high + Wrapping(C1)).0 >> 32) as i32)
    }).collect::<Result<Vec<i32>, SquadOvError>>()
}