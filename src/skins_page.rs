//! The Skins tab. For now its cape part: the capes the Minecraft account that plays owns, one
//! click to wear one or none (`account::capes::wear`, seen by everyone, in every launcher and on
//! every server), and the player's OptiFine cape, looked up by name, with OptiFine's cape editor
//! a click away.
//!
//! Every request runs on a worker and comes back as a message (`Msg::Capes`, `CapePicture`,
//! `OptifineCape`, `OptifineEditor`); a listing or a change answers under the number it was
//! asked with, so an answer for an account no longer shown, or one a later request replaced, is
//! dropped.
use super::*;
use crate::frame::Reach as _;
use crate::paint::palette;
use gpui::{img, rgb, RenderImage};
use std::cell::RefCell;
use std::sync::Arc;
use sunrise_launcher::account::capes::{self, Cape};

use capes::Face as Texels;

/// What OptiFine's cape server said about a name: its cape, none, or why it couldn't be asked.
type OptifineAnswer = Result<Option<Texels>, String>;

/// The side of a cape tile's picture: the face is 10 by 16, drawn 6 points a texel.
const FACE_W: f32 = 60.;
const FACE_H: f32 = 96.;
const TILE_W: f32 = 116.;

#[derive(Default)]
pub(crate) struct Skins {
    /// The account the capes are of.
    uuid: Option<String>,
    /// The number of the last listing or change asked for: only its answer is kept.
    asked: u64,
    /// None while the first listing is under way.
    capes: Option<Result<Vec<Cape>, String>>,
    /// The change under way: the cape to wear, or none (`Some(None)`).
    wearing: Option<Option<String>>,
    /// Why the last change didn't happen.
    refused: Option<String>,
    /// The pictures asked for, by URL; none until one arrives (or when it can't be had).
    pictures: HashMap<String, Option<Texels>>,
    /// The name the OptiFine cape was asked for, and the answer once it is in.
    optifine: Option<(String, Option<OptifineAnswer>)>,
    /// The editor's session join is under way.
    opening_editor: bool,
    editor_refused: Option<String>,
    /// Pictures as the window draws them, by URL (or the OptiFine name) and scale.
    baked: RefCell<HashMap<(String, u32), Arc<RenderImage>>>,
}

impl Skins {
    /// A listing or a change answered.
    pub(crate) fn answered(
        &mut self,
        uuid: &str,
        asked: u64,
        result: Result<Vec<Cape>, String>,
    ) -> bool {
        if self.uuid.as_deref() != Some(uuid) || asked != self.asked {
            return false;
        }
        let changing = self.wearing.take().is_some();
        match result {
            Ok(list) => self.capes = Some(Ok(list)),
            // A refused change keeps the capes as they were and says why.
            Err(why) if changing && matches!(self.capes, Some(Ok(_))) => self.refused = Some(why),
            Err(why) => self.capes = Some(Err(why)),
        }
        true
    }

    /// The pictures of `list` not yet asked for.
    fn unasked(&self, list: &[Cape]) -> Vec<String> {
        list.iter()
            .map(|c| c.url.clone())
            .filter(|url| !self.pictures.contains_key(url))
            .collect()
    }

    pub(crate) fn picture(&mut self, url: String, face: Option<Texels>) {
        self.baked.get_mut().retain(|(key, _), _| *key != url);
        self.pictures.insert(url, face);
    }

    pub(crate) fn optifine_cape(&mut self, name: &str, found: OptifineAnswer) {
        if let Some((asked, answer)) = &mut self.optifine {
            if asked == name {
                *answer = Some(found);
                self.baked.get_mut().retain(|(key, _), _| key != name);
            }
        }
    }

    /// The face `texels` as the window draws it, `pixel` points a texel, kept by `key`.
    fn drawn(&self, key: &str, texels: &Texels, pixel: f32, scale: f32) -> Arc<RenderImage> {
        let (face, w, h) = texels;
        self.baked
            .borrow_mut()
            .entry((key.to_owned(), scale.to_bits()))
            .or_insert_with(|| art::bake(face, *w, *h, pixel, scale))
            .clone()
    }
}

impl Launcher {
    /// The account whose capes the page shows: the one that plays (the selected one, else the
    /// first), as the game picks it.
    fn cape_account(&self) -> Option<account::Account> {
        self.accounts
            .selected()
            .or_else(|| self.accounts.accounts.first())
            .cloned()
    }

    /// The page opened: its capes are asked for when they are another account's, or the last
    /// listing failed.
    pub(crate) fn skins_open(&mut self) {
        let Some(account) = self.cape_account() else {
            return;
        };
        let same = self.skins.uuid.as_deref() == Some(&account.uuid);
        if !same || matches!(self.skins.capes, Some(Err(_))) {
            self.capes_ask(&account, None, false);
        }
        let name_asked =
            self.skins.optifine.as_ref().is_some_and(|(name, answer)| {
                *name == account.name && !matches!(answer, Some(Err(_)))
            });
        if !name_asked {
            self.optifine_ask(&account.name);
        }
    }

    /// Lists the capes of `account`, or (`change`) wears one of them or none first.
    fn capes_ask(&mut self, account: &account::Account, wear: Option<String>, change: bool) {
        if self.skins.uuid.as_deref() != Some(&account.uuid) {
            self.skins = skins_page::Skins {
                uuid: Some(account.uuid.clone()),
                pictures: std::mem::take(&mut self.skins.pictures),
                optifine: self.skins.optifine.take(),
                ..Default::default()
            };
        }
        self.skins.asked += 1;
        self.skins.refused = None;
        if change {
            self.skins.wearing = Some(wear.clone());
        } else {
            self.skins.capes = None;
        }
        let (asked, account, tx) = (self.skins.asked, account.clone(), self.tx.clone());
        std::thread::spawn(move || {
            let ep = account::Endpoints::live();
            let result = account::token_for(
                &ep,
                account::client_id().as_deref(),
                account::store(),
                &account,
            )
            .and_then(|token| match change {
                true => capes::wear(&ep, &token, wear.as_deref()),
                false => capes::list(&ep, &token),
            });
            let _ = tx.send(Msg::Capes(account.uuid, asked, result));
        });
    }

    fn optifine_ask(&mut self, name: &str) {
        self.skins.optifine = Some((name.to_owned(), None));
        let (name, tx) = (name.to_owned(), self.tx.clone());
        std::thread::spawn(move || {
            let found = capes::optifine(capes::OPTIFINE, &name).and_then(|png| {
                png.map(|png| {
                    capes::front(&png)
                        .ok_or_else(|| "OptiFine's cape picture can't be read.".to_string())
                })
                .transpose()
            });
            let _ = tx.send(Msg::OptifineCape(name, found));
        });
    }

    /// A listing or a change answered (`Msg::Capes`): the pictures not yet asked for are, and a
    /// change shows on the account chip's account at once.
    pub(crate) fn capes_answered(
        &mut self,
        uuid: String,
        asked: u64,
        result: Result<Vec<Cape>, String>,
    ) {
        if !self.skins.answered(&uuid, asked, result) {
            return;
        }
        let Some(Ok(list)) = &self.skins.capes else {
            return;
        };
        let worn = list.iter().find(|c| c.active).map(|c| c.url.clone());
        if let Some(a) = self.accounts.accounts.iter_mut().find(|a| a.uuid == uuid) {
            a.cape = worn;
        }
        for url in self.skins.unasked(list) {
            self.skins.pictures.insert(url.clone(), None);
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let face = capes::picture(&url).and_then(|png| capes::front(&png));
                let _ = tx.send(Msg::CapePicture(url, face));
            });
        }
    }

    fn cape_wear(&mut self, id: Option<String>) {
        if self.skins.wearing.is_some() {
            return;
        }
        if let Some(account) = self.cape_account() {
            self.capes_ask(&account, id, true);
        }
    }

    fn optifine_editor_open(&mut self) {
        let Some(account) = self.cape_account() else {
            return;
        };
        if self.skins.opening_editor {
            return;
        }
        self.skins.opening_editor = true;
        self.skins.editor_refused = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let ep = account::Endpoints::live();
            let result = account::token_for(
                &ep,
                account::client_id().as_deref(),
                account::store(),
                &account,
            )
            .and_then(|token| capes::optifine_editor(capes::SESSION, &token, &account));
            let _ = tx.send(Msg::OptifineEditor(result));
        });
    }

    /// OptiFine's editor is ready to open (or can't be).
    pub(crate) fn optifine_editor_answered(
        &mut self,
        result: Result<String, String>,
        cx: &mut App,
    ) {
        self.skins.opening_editor = false;
        match result {
            Ok(url) => cx.open_url(&url),
            Err(why) => self.skins.editor_refused = Some(why),
        }
    }

    pub(crate) fn skins_page(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let page = div()
            .id("skins-page")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .p(px(24.))
            .gap(px(24.))
            .overflow_y_scroll()
            .track_scroll(&crate::frame::scrolls("skins-page"))
            .child(
                div()
                    .text_size(px(tokens::TITLE))
                    .font_weight(FontWeight::BOLD)
                    .child(tr("Skins")),
            );
        let Some(account) = self.cape_account() else {
            return page.child(
                crate::home::card()
                    .p(px(20.))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(12.))
                    .child(crate::settings::hint(tr(
                        "Sign in with the Microsoft account that owns Minecraft: Java Edition to choose its cape.",
                    )))
                    .child(crate::frame::button("skins-sign-in", tr("Sign in with Microsoft")).on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.sign_in_to_play();
                            cx.notify();
                        }),
                    )),
            );
        };
        page.child(self.minecraft_capes(cx))
            .child(self.optifine_section(&account, cx))
    }

    fn section(title: &str, about: &str) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(crate::home::heading(tr(title)))
            .child(crate::settings::hint(tr(about)))
    }

    fn minecraft_capes(&mut self, cx: &mut Context<Self>) -> Div {
        let section = div().flex().flex_col().gap(px(12.)).child(Self::section(
            "Minecraft capes",
            "The capes your Minecraft account owns. Everyone sees the one you wear, in every launcher and on every server.",
        ));
        let list = match &self.skins.capes {
            None => return section.child(crate::settings::hint(tr("Looking up your capes…"))),
            Some(Err(why)) => {
                return section.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(crate::settings::warning(why.clone()))
                        .child(
                            crate::frame::button("capes-retry", tr("Try again")).on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    if let Some(account) = this.cape_account() {
                                        this.capes_ask(&account, None, false);
                                    }
                                    cx.notify();
                                }),
                            ),
                        ),
                )
            }
            Some(Ok(list)) => list.clone(),
        };
        let none_worn = list.iter().all(|c| !c.active);
        let mut tiles = div().flex().flex_wrap().gap(px(12.)).child(self.cape_tile(
            None,
            tr("No cape").into(),
            none_worn,
            cx,
        ));
        for cape in &list {
            tiles =
                tiles.child(self.cape_tile(Some(cape), cape.alias.clone().into(), cape.active, cx));
        }
        let mut section = section.child(tiles);
        if list.is_empty() {
            section = section.child(crate::settings::hint(tr(
                "This account owns no capes. Mojang gives them for events and promotions.",
            )));
        }
        if let Some(why) = &self.skins.refused {
            section = section.child(crate::settings::warning(why.clone()));
        }
        section
    }

    /// One cape to click, or no cape (`cape` none).
    fn cape_tile(
        &self,
        cape: Option<&Cape>,
        label: SharedString,
        worn: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let p = palette();
        let id = cape.map(|c| c.id.clone());
        let changing = self.skins.wearing.as_ref() == Some(&id);
        let busy = self.skins.wearing.is_some();
        let face = cape
            .and_then(|c| {
                self.skins
                    .pictures
                    .get(&c.url)?
                    .as_ref()
                    .map(|f| (c.url.as_str(), f))
            })
            .map(|(url, texels)| {
                let pixel = FACE_W / texels.1 as f32;
                img(self.skins.drawn(url, texels, pixel, self.scale))
                    .w(px(FACE_W))
                    .h(px(FACE_H))
            });
        let picture = div()
            .flex_none()
            .w(px(FACE_W))
            .h(px(FACE_H))
            .flex()
            .items_center()
            .justify_center()
            .rounded(crate::tokens::r(crate::tokens::R_SMALL))
            .bg(self.theme.well)
            .map(|d| match face {
                Some(face) => d.child(face),
                None if cape.is_none() => d.child(blocks::icon("close", 22., self.theme.muted)),
                None => d,
            });
        let status = if changing {
            Some((tr("Changing…"), p.muted))
        } else if worn {
            Some((tr("Wearing"), p.accent_ink))
        } else {
            None
        };
        div()
            .id(SharedString::from(format!(
                "cape-{}",
                id.as_deref().unwrap_or("none")
            )))
            .reach_edge()
            .w(px(TILE_W))
            .p(px(10.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .rounded(crate::tokens::r(crate::tokens::R_CARD))
            .border_1()
            .border_color(rgb(if worn { p.accent } else { p.divider }))
            .bg(palette().card())
            .when(!worn && !busy, |d| {
                d.cursor_pointer().hover(|s| s.bg(rgb(p.hover)))
            })
            .tooltip(crate::glass_ui::tip(label.clone()))
            .child(picture)
            .child(
                div()
                    .w_full()
                    .text_center()
                    .text_size(px(tokens::SMALL))
                    .font_weight(FontWeight::SEMIBOLD)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label),
            )
            .child(
                div()
                    .h(px(16.))
                    .text_size(px(tokens::CAPTION))
                    .when_some(status, |d, (text, colour)| {
                        d.text_color(rgb(colour)).child(text)
                    }),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if !worn {
                    this.cape_wear(id.clone());
                    cx.notify();
                }
            }))
    }

    fn optifine_section(&mut self, account: &account::Account, cx: &mut Context<Self>) -> Div {
        let section = div().flex().flex_col().gap(px(12.)).child(Self::section(
            "OptiFine cape",
            "OptiFine keeps this cape under your name. Players who use OptiFine see it.",
        ));
        let answer = self
            .skins
            .optifine
            .as_ref()
            .filter(|(name, _)| *name == account.name)
            .and_then(|(_, answer)| answer.as_ref());
        let face = match answer {
            Some(Ok(Some(texels))) => Some(
                img(self
                    .skins
                    .drawn(&account.name, texels, FACE_W / texels.1 as f32, self.scale))
                .w(px(FACE_W))
                .h(px(FACE_H)),
            ),
            _ => None,
        };
        let (title, about) = match answer {
            None => (tr("Looking up your OptiFine cape…").to_string(), String::new()),
            Some(Ok(Some(_))) => (
                trf("{name}'s OptiFine cape", &[("name", &account.name)]),
                tr("Change its look on optifine.net: Sunrise signs you in there with your Minecraft account.")
                    .to_string(),
            ),
            Some(Ok(None)) => (
                trf("{name} has no OptiFine cape", &[("name", &account.name)]),
                tr("OptiFine gives one to players who donate to it.").to_string(),
            ),
            Some(Err(why)) => (tr("Couldn't look up your OptiFine cape").to_string(), why.clone()),
        };
        let opening = self.skins.opening_editor;
        let action = match answer {
            Some(Ok(Some(_))) => Some(
                crate::frame::button(
                    "optifine-edit",
                    tr(if opening {
                        "Opening…"
                    } else {
                        "Edit on OptiFine"
                    }),
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.optifine_editor_open();
                    cx.notify();
                })),
            ),
            Some(Ok(None)) => Some(
                crate::frame::button("optifine-get", tr("Get an OptiFine cape"))
                    .on_click(|_, _, cx| cx.open_url(capes::OPTIFINE_DONATE)),
            ),
            Some(Err(_)) => Some(
                crate::frame::button("optifine-retry", tr("Try again")).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        if let Some(account) = this.cape_account() {
                            this.optifine_ask(&account.name);
                        }
                        cx.notify();
                    },
                )),
            ),
            None => None,
        };
        let card = crate::home::card()
            .p(px(14.))
            .flex()
            .items_center()
            .gap(px(16.))
            .child(
                div()
                    .flex_none()
                    .w(px(FACE_W))
                    .h(px(FACE_H))
                    .rounded(crate::tokens::r(crate::tokens::R_SMALL))
                    .bg(self.theme.well)
                    .when_some(face, |d, face| d.child(face)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                    .when(!about.is_empty(), |d| {
                        d.child(if matches!(answer, Some(Err(_))) {
                            crate::settings::warning(about)
                        } else {
                            crate::settings::hint(about)
                        })
                    })
                    .when_some(self.skins.editor_refused.clone(), |d, why| {
                        d.child(crate::settings::warning(why))
                    }),
            )
            .when_some(action, |d, action| d.child(div().flex_none().child(action)));
        section.child(card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cape(id: &str, active: bool) -> Cape {
        Cape {
            id: id.into(),
            alias: id.into(),
            url: format!("http://textures.minecraft.net/texture/{id}"),
            active,
        }
    }

    fn shown(uuid: &str, asked: u64) -> Skins {
        Skins {
            uuid: Some(uuid.into()),
            asked,
            ..Default::default()
        }
    }

    #[test]
    fn only_the_last_answer_for_the_account_shown_is_kept() {
        let mut skins = shown("a", 2);
        assert!(!skins.answered("a", 1, Ok(vec![cape("old", true)])));
        assert!(!skins.answered("b", 2, Ok(vec![cape("other", true)])));
        assert!(skins.capes.is_none());
        assert!(skins.answered("a", 2, Ok(vec![cape("c1", true)])));
        assert_eq!(skins.capes, Some(Ok(vec![cape("c1", true)])));
    }

    #[test]
    fn a_refused_change_keeps_the_capes_and_says_why() {
        let mut skins = shown("a", 1);
        skins.answered("a", 1, Ok(vec![cape("c1", true), cape("c2", false)]));
        skins.asked = 2;
        skins.wearing = Some(Some("c2".into()));
        assert!(skins.answered("a", 2, Err("Too many".into())));
        assert_eq!(
            skins.capes,
            Some(Ok(vec![cape("c1", true), cape("c2", false)]))
        );
        assert_eq!(skins.refused.as_deref(), Some("Too many"));
        assert!(skins.wearing.is_none(), "the next change can be asked for");
        // A listing that fails says so in place of the capes.
        skins.asked = 3;
        skins.answered("a", 3, Err("No network".into()));
        assert_eq!(skins.capes, Some(Err("No network".into())));
    }

    #[test]
    fn each_picture_is_asked_for_once() {
        let mut skins = shown("a", 1);
        let list = [cape("c1", true), cape("c2", false)];
        assert_eq!(skins.unasked(&list).len(), 2);
        skins.picture(list[0].url.clone(), None);
        assert_eq!(skins.unasked(&list), [list[1].url.clone()]);
    }
}
