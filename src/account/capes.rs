//! Capes: the ones a Minecraft account owns and which of them it wears (the Minecraft services'
//! profile), and the player's OptiFine cape.
//!
//! A Minecraft cape is given by Mojang and listed on the profile, `ACTIVE` or `INACTIVE`; the
//! account wears one (`PUT /minecraft/profile/capes/active` with its id) or none (`DELETE` on the
//! same path), and every client and server shows that one, since it travels in the signed profile.
//!
//! An OptiFine cape is a picture OptiFine's cape server keeps under the player's name
//! (`http://s.optifine.net/capes/<name>.png`, plain HTTP: the server has no HTTPS). Only clients
//! that ask that server show it. Its look is changed on optifine.net, whose editor trusts the
//! player after a Mojang session join, as OptiFine's own "Open Cape Editor" does.
use super::{bearer, json, request_to, send, str_field, Account, Body, Endpoints, Value};
use std::io::Read;
use std::time::Duration;

/// Mojang's session server, which OptiFine's cape editor asks whether the player joined.
pub const SESSION: &str = "https://sessionserver.mojang.com";
/// OptiFine's cape server.
pub const OPTIFINE: &str = "http://s.optifine.net";
/// Where an OptiFine cape is got.
pub const OPTIFINE_DONATE: &str = "https://optifine.net/donate";
/// The most a cape picture may weigh: real ones are a few kilobytes.
const MAX_PICTURE: u64 = 256 * 1024;

/// A cape's outer face as [`front`] cuts it: texels row by row as RGBA, width, height.
pub type Face = (Vec<[u8; 4]>, usize, usize);

/// One cape the account owns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cape {
    pub id: String,
    /// Its name ("Migrator").
    pub alias: String,
    /// Its picture on textures.minecraft.net.
    pub url: String,
    /// The account wears it.
    pub active: bool,
}

/// The capes a profile (`/minecraft/profile`) lists, in its order.
pub fn owned(profile: &Value) -> Vec<Cape> {
    profile
        .get("capes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| {
            Some(Cape {
                id: str_field(c, "id")?,
                alias: str_field(c, "alias").unwrap_or_default(),
                url: str_field(c, "url").filter(|u| is_texture(u))?,
                active: c.get("state").and_then(Value::as_str) == Some("ACTIVE"),
            })
        })
        .collect()
}

/// A picture on Mojang's texture server, the only place a Minecraft cape's picture is fetched
/// from (as the game, after authlib, does).
pub fn is_texture(url: &str) -> bool {
    [
        "http://textures.minecraft.net/texture/",
        "https://textures.minecraft.net/texture/",
    ]
    .iter()
    .any(|base| {
        url.strip_prefix(base)
            .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric()))
    })
}

/// The capes the account with `token` owns.
pub fn list(ep: &Endpoints, token: &str) -> Result<Vec<Cape>, String> {
    let (status, v) = send(
        bearer(
            request_to("GET", &format!("{}/minecraft/profile", ep.minecraft)),
            token,
        ),
        None,
        "the Minecraft services",
    )?;
    answer(status, &v)
}

/// Wears the cape `id`, or none; the capes as they are after it.
pub fn wear(ep: &Endpoints, token: &str, id: Option<&str>) -> Result<Vec<Cape>, String> {
    let url = format!("{}/minecraft/profile/capes/active", ep.minecraft);
    let (status, v) = match id {
        Some(id) => send(
            bearer(request_to("PUT", &url), token),
            Some(Body::Json(json!({ "capeId": id }))),
            "the Minecraft services",
        )?,
        None => send(
            bearer(request_to("DELETE", &url), token),
            None,
            "the Minecraft services",
        )?,
    };
    answer(status, &v)
}

fn answer(status: u16, v: &Value) -> Result<Vec<Cape>, String> {
    match status {
        200 => Ok(owned(v)),
        401 => Err(super::SIGN_IN_AGAIN.into()),
        400 | 404 => Err("This account doesn't own that cape.".into()),
        429 => Err("Too many cape changes in a short time. Wait a minute, then try again.".into()),
        _ => Err(format!(
            "The Minecraft services couldn't change the cape (error {status}). Try again later."
        )),
    }
}

/// A name OptiFine's cape server can have a cape under: a Minecraft name, 1 to 16 letters,
/// digits and underscores. Nothing else is ever put in its URL.
pub fn optifine_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The OptiFine cape of `name` on the cape server at `base` ([`OPTIFINE`]): its PNG, or none when
/// the player has none.
pub fn optifine(base: &str, name: &str) -> Result<Option<Vec<u8>>, String> {
    if !optifine_name(name) {
        return Ok(None);
    }
    let url = format!("{base}/capes/{name}.png");
    let agent = crate::proxy::agent(
        ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .user_agent(&format!("sunrise-launcher/{}", crate::backend::VERSION)),
        &url,
    )
    .build();
    let response = match agent.get(&url).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(ureq::Error::Status(status, _)) => {
            return Err(format!(
                "OptiFine's cape server answered with error {status}."
            ))
        }
        Err(ureq::Error::Transport(_)) => {
            return Err("Could not reach OptiFine's cape server.".into())
        }
    };
    let bytes = read_picture(response.into_reader())
        .ok_or("OptiFine's cape server sent something that isn't a cape picture.")?;
    Ok(Some(bytes))
}

/// A Minecraft cape's picture from [`is_texture`]'s server; none when it can't be had.
pub fn picture(url: &str) -> Option<Vec<u8>> {
    if !is_texture(url) {
        return None;
    }
    let response = crate::proxy::agent(
        ureq::AgentBuilder::new().timeout(Duration::from_secs(10)),
        url,
    )
    .build()
    .get(url)
    .call()
    .ok()?;
    read_picture(response.into_reader())
}

/// At most [`MAX_PICTURE`] bytes that decode as a PNG.
fn read_picture(reader: impl Read) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(MAX_PICTURE + 1).read_to_end(&mut bytes).ok()?;
    let fits = bytes.len() as u64 <= MAX_PICTURE;
    (fits && image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).is_ok())
        .then_some(bytes)
}

/// The cape's outer face, as a player behind it sees it, row by row as RGBA: 10 by 16 texels of
/// the 64 by 32 layout (at 1, 1), or that many times 2ⁿ for a bigger picture. OptiFine's 46 by 22
/// pictures and the old 22 by 17 ones keep the face where the 64 by 32 layout has it; a picture is
/// laid out on the smallest such canvas it fits, as OptiFine does.
pub fn front(png: &[u8]) -> Option<Face> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 || w > 2048 || h > 1024 {
        return None;
    }
    let mut k = 1;
    while 64 * k < w || 32 * k < h {
        k *= 2;
    }
    let (fw, fh) = (10 * k, 16 * k);
    if k + fw > w || k + fh > h {
        return None;
    }
    let texels = (k..k + fh)
        .flat_map(|y| (k..k + fw).map(move |x| (x, y)))
        .map(|(x, y)| image.get_pixel(x, y).0)
        .collect();
    Some((texels, fw as usize, fh as usize))
}

/// Opens OptiFine's cape editor for `account`: a session join on Mojang's session server at
/// `session` ([`SESSION`]) with a fresh server id, then the editor's address, which carries the
/// account's id, name and that server id for OptiFine to check the join with. The token goes to
/// Mojang only, never to OptiFine.
pub fn optifine_editor(session: &str, token: &str, account: &Account) -> Result<String, String> {
    if !optifine_name(&account.name) {
        return Err("This name can't have an OptiFine cape.".into());
    }
    let mut random = [0u8; 20];
    getrandom::fill(&mut random).map_err(|e| e.to_string())?;
    let server_id: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let (status, _) = send(
        request_to("POST", &format!("{session}/session/minecraft/join")),
        Some(Body::Json(json!({
            "accessToken": token,
            "selectedProfile": account.uuid,
            "serverId": server_id,
        }))),
        "Mojang's session server",
    )?;
    match status {
        200..=299 => Ok(format!(
            "https://optifine.net/capeChange?u={}&n={}&s={server_id}",
            account.uuid, account.name
        )),
        401 => Err(super::SIGN_IN_AGAIN.into()),
        // What a join gets when the account may not play online (Xbox privacy settings, a ban).
        403 => Err("Mojang's session server refused the sign-in: this account may not play \
                    online (check its Xbox privacy settings)."
            .into()),
        _ => Err(format!(
            "Mojang's session server couldn't confirm the sign-in (error {status}). Try again later."
        )),
    }
}
