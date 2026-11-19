//! What the Mods screens show, worked out from the registry index and the installed list:
//! pure data in and out, no files, no network, no window. Everything here only reads.
use std::collections::{BTreeSet, HashMap, HashSet};
use sunrise_launcher::browser::installed::Entry;
use sunrise_launcher::i18n::{tr, trf};
use sunrise_launcher::native_mods;
use sunrise_launcher_smod::index::{self, Index};
use sunrise_launcher_smod::manifest::Side;

/// What a mod is made of, as the registry knows it: a native source bundle, or content only (the
/// registry's `mod` kind is "a `.smod` of content only"; it has no wasm kind to show).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum Make {
    Native,
    Content,
}

impl Make {
    pub(super) const ALL: [Make; 2] = [Make::Native, Make::Content];

    pub(super) fn label(self) -> &'static str {
        match self {
            Make::Native => tr("Native"),
            Make::Content => tr("Content"),
        }
    }
}

/// Whether the version the player would get runs on the build they play.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Fit {
    Fits,
    /// Versions exist, none for this build: the newest build one was made for.
    OtherBuild(String),
    /// Taken down, or asks for something this Sunrise doesn't have.
    Refused(String),
}

impl Fit {
    pub(super) fn fits(&self) -> bool {
        matches!(self, Fit::Fits)
    }

    pub(super) fn reason(&self) -> Option<String> {
        match self {
            Fit::Fits => None,
            Fit::OtherBuild(game) => Some(trf("Made for {game}", &[("game", game)])),
            Fit::Refused(why) => Some(why.clone()),
        }
    }
}

/// One mod in Discover.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Card {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) authors: String,
    pub(super) summary: String,
    pub(super) downloads: u64,
    pub(super) make: Make,
    pub(super) categories: Vec<String>,
    pub(super) icon: Option<String>,
    /// The newest version's day, for sorting.
    pub(super) updated: String,
    pub(super) first: String,
    pub(super) fit: Fit,
    /// A native mod with no prebuilt binary for this game: it is built on this computer.
    pub(super) builds_locally: bool,
}

pub(super) fn make_of(p: &index::Project) -> Option<Make> {
    match p.kind {
        index::Kind::NativeMod => Some(Make::Native),
        index::Kind::Mod => Some(Make::Content),
        _ => None,
    }
}

pub(super) fn fit_of(index: &Index, p: &index::Project, game: &str) -> Fit {
    if index.installable(p, game).is_some() {
        return Fit::Fits;
    }
    let for_game: Vec<&index::Version> = p.versions.iter().filter(|v| v.game == game).collect();
    if for_game.is_empty() {
        return Fit::OtherBuild(
            p.versions
                .first()
                .map(|v| v.game.clone())
                .unwrap_or_default(),
        );
    }
    if let Some(r) = for_game
        .iter()
        .find_map(|v| index.revoked(&p.id, &v.file.sha256))
    {
        return Fit::Refused(trf("Taken down: {reason}", &[("reason", &r.reason)]));
    }
    Fit::Refused(tr("Made for Sunrise's earlier mod system").to_string())
}

/// Every mod and native mod of the registry, for the build `game`. Packs are left out.
pub(super) fn cards(index: &Index, game: &str) -> Vec<Card> {
    index
        .projects
        .iter()
        .filter_map(|p| {
            let make = make_of(p)?;
            Some(Card {
                id: p.id.clone(),
                name: p.name.clone(),
                authors: p.authors.join(", "),
                summary: p.summary.clone(),
                downloads: p.downloads,
                make,
                categories: p.categories.clone(),
                icon: p.icon.clone(),
                updated: p
                    .versions
                    .iter()
                    .map(|v| v.published.as_str())
                    .max()
                    .unwrap_or("")
                    .into(),
                first: p
                    .versions
                    .iter()
                    .map(|v| v.published.as_str())
                    .min()
                    .unwrap_or("")
                    .into(),
                fit: fit_of(index, p, game),
                // Only claimed when the registry lists no prebuilt at all: which one fits this
                // computer is decided at install, by build and platform.
                builds_locally: make == Make::Native
                    && index
                        .installable(p, game)
                        .is_some_and(|v| v.native_binaries.is_empty()),
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sort {
    Downloads,
    Updated,
    Newest,
    Name,
}

impl Sort {
    pub(super) const ALL: [Sort; 4] = [Sort::Downloads, Sort::Updated, Sort::Newest, Sort::Name];

    pub(super) fn label(self) -> &'static str {
        match self {
            Sort::Downloads => tr("Most downloaded"),
            Sort::Updated => tr("Recently updated"),
            Sort::Newest => tr("Newest"),
            Sort::Name => tr("Name"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Filters {
    pub(super) text: String,
    pub(super) category: Option<String>,
    pub(super) fits_only: bool,
    pub(super) make: Option<Make>,
    pub(super) sort: Sort,
}

impl Default for Filters {
    fn default() -> Self {
        Self {
            text: String::new(),
            category: None,
            fits_only: true,
            make: None,
            sort: Sort::Downloads,
        }
    }
}

/// Indexes into `cards` that pass `f`, in the order asked for.
pub(super) fn discover(cards: &[Card], f: &Filters) -> Vec<usize> {
    let words: Vec<String> = f.text.split_whitespace().map(str::to_lowercase).collect();
    let mut shown: Vec<usize> = (0..cards.len())
        .filter(|&i| {
            let c = &cards[i];
            let hay = format!("{} {} {} {}", c.id, c.name, c.summary, c.authors).to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
                && f.category.as_ref().is_none_or(|k| c.categories.contains(k))
                && f.make.is_none_or(|m| c.make == m)
                && (!f.fits_only || c.fit.fits())
        })
        .collect();
    match f.sort {
        Sort::Downloads => shown.sort_by_key(|&i| std::cmp::Reverse(cards[i].downloads)),
        Sort::Updated => shown.sort_by(|&a, &b| {
            cards[b]
                .updated
                .cmp(&cards[a].updated)
                .then(cards[b].downloads.cmp(&cards[a].downloads))
        }),
        Sort::Newest => shown.sort_by(|&a, &b| {
            cards[b]
                .first
                .cmp(&cards[a].first)
                .then(cards[b].downloads.cmp(&cards[a].downloads))
        }),
        Sort::Name => shown.sort_by_key(|&i| cards[i].name.to_lowercase()),
    }
    shown
}

/// The categories the mods carry, most used first.
pub(super) fn categories(cards: &[Card]) -> Vec<String> {
    let mut uses: HashMap<&str, usize> = HashMap::new();
    for c in cards {
        for k in &c.categories {
            *uses.entry(k).or_default() += 1;
        }
    }
    let mut all: Vec<(&str, usize)> = uses.into_iter().collect();
    all.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    all.into_iter().map(|(k, _)| k.to_owned()).collect()
}

/// `6814` → `6.8k downloads`.
pub(super) fn downloads(n: u64) -> String {
    let n = match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}k", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    };
    trf("{count} downloads", &[("count", &n)])
}

pub(super) fn size(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.1} KiB", bytes as f64 / 1024.),
        _ => format!("{:.1} MiB", bytes as f64 / 1_048_576.),
    }
}

pub(super) fn side_label(side: Option<&Side>) -> &'static str {
    match side {
        None | Some(Side::Client) => tr("Not needed on servers"),
        Some(Side::Both) => tr("Servers need it too"),
        Some(Side::Server) => tr("Server only"),
        Some(_) => tr("Server only"),
    }
}

// ---------------------------------------------------------------- the project page

#[derive(Clone, Debug, PartialEq)]
pub(super) struct VersionLine {
    pub(super) number: String,
    pub(super) published: String,
    pub(super) game: String,
    pub(super) fit: Fit,
    pub(super) size: u64,
}

/// Everything the project page shows, read from the index in one go.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Page {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) authors: Vec<String>,
    pub(super) description: String,
    pub(super) license: String,
    pub(super) source: String,
    pub(super) downloads: u64,
    pub(super) updated: String,
    pub(super) categories: Vec<String>,
    pub(super) screenshots: Vec<String>,
    pub(super) icon: Option<String>,
    pub(super) make: Make,
    pub(super) side: &'static str,
    pub(super) versions: Vec<VersionLine>,
    pub(super) fit: Fit,
    /// The newest version that fits: what Add installs.
    pub(super) will_install: Option<VersionLine>,
    /// What the registry says about who made it and what it may do (`trust_and_capabilities`).
    pub(super) trust: Vec<String>,
    /// Mods the newest version lists as part of itself (a registry modpack's rule; a mod rarely has any).
    pub(super) requires: Vec<(String, String)>,
}

pub(super) fn page(index: &Index, id: &str, game: &str, trust: Vec<String>) -> Option<Page> {
    let p = index.project(id)?;
    let make = make_of(p)?;
    let lines: Vec<VersionLine> = p
        .versions
        .iter()
        .map(|v| VersionLine {
            number: v.version.clone(),
            published: v.published.clone(),
            game: v.game.clone(),
            fit: match (v.game == game, index.revoked(&p.id, &v.file.sha256)) {
                (_, Some(r)) => Fit::Refused(trf("Taken down: {reason}", &[("reason", &r.reason)])),
                _ if !v.capabilities.unknown().is_empty() => {
                    Fit::Refused(tr("Made for Sunrise's earlier mod system").to_string())
                }
                (true, None) => Fit::Fits,
                (false, None) => Fit::OtherBuild(v.game.clone()),
            },
            size: v.file.size,
        })
        .collect();
    let will_install = index
        .installable(p, game)
        .and_then(|v| lines.iter().find(|l| l.number == v.version).cloned());
    Some(Page {
        id: p.id.clone(),
        name: p.name.clone(),
        authors: p.authors.clone(),
        description: p.description.clone(),
        license: p.license.clone(),
        source: p.source.clone(),
        downloads: p.downloads,
        updated: lines
            .iter()
            .map(|l| l.published.clone())
            .max()
            .unwrap_or_default(),
        categories: p.categories.clone(),
        screenshots: p.screenshots.clone(),
        icon: p.icon.clone(),
        make,
        side: side_label(p.versions.first().and_then(|v| v.side.as_ref())),
        fit: fit_of(index, p, game),
        will_install,
        versions: lines,
        trust,
        requires: p
            .versions
            .first()
            .map(|v| {
                v.requires
                    .iter()
                    .map(|r| {
                        let name = index
                            .project(&r.id)
                            .map_or(r.id.clone(), |m| m.name.clone());
                        (name, r.version.clone())
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

// ---------------------------------------------------------------- installed

/// What stands between an installed mod and working, shown on its row.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Need {
    /// Taken down by the registry: it can't be turned on.
    TakenDown(String),
    /// A native mod, on, whose build for this computer isn't made yet.
    Build,
    /// A native mod that can't run until the player allows native code.
    Permission,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Row {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) authors: String,
    pub(super) make: Option<Make>,
    pub(super) version: String,
    pub(super) on: bool,
    pub(super) need: Option<Need>,
    /// The version an update would install.
    pub(super) update: Option<String>,
    /// Installed because another mod needed it.
    pub(super) dependency: bool,
    /// The player chose this version (`sunrise::chosen`): a batch leaves it alone.
    pub(super) pinned: bool,
    /// Not installed by the browser: it switches on and off, but has no updates or removal here.
    pub(super) local: Option<Local>,
    pub(super) icon: Option<String>,
    pub(super) entry: Option<Entry>,
    /// It can't run on the game this setup plays: the build left it out.
    pub(super) unfit: Option<Unfit>,
    /// The build left it out for another reason (it didn't build, it needs a mod that was left
    /// out, it clashes with another): what the player reads, in a sentence.
    pub(super) left_out: Option<String>,
}

/// Why a mod is not on the game build its setup plays, in parts the row says in its own words.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Unfit {
    /// The build played, as people read it ("Nov 17").
    pub(super) on: String,
    /// The build it was made for: its version and its name ("Nov 16"), when known.
    pub(super) made_for: Option<(String, String)>,
    /// Each thing the game changed that it uses, for its author.
    pub(super) changes: Vec<String>,
}

/// Marks the rows of mods the last build left out: those the game was made without (another
/// version, or not fit for it) get their [`Unfit`] words, every other reason (it didn't build,
/// it needs a mod that was left out, it clashes) its sentence. A row left out has nothing to
/// build, so it no longer asks for a build. `on` is the build played, `name` how a build tag is
/// read by people.
pub(super) fn mark_left_out(
    rows: &mut [Row],
    left: &[native_mods::LeftOut],
    on: &str,
    name: impl Fn(&str) -> String,
) {
    for row in rows.iter_mut().filter(|r| r.on) {
        let Some(l) = left
            .iter()
            .find(|l| l.id == row.id && l.version == row.version)
        else {
            continue;
        };
        row.need = None;
        match l.why {
            native_mods::Why::Unfit(_) | native_mods::Why::OtherVersion => {
                let detail = l.detail.clone().unwrap_or_default();
                row.unfit = Some(Unfit {
                    on: on.to_owned(),
                    made_for: detail.made_for.map(|tag| (tag.clone(), name(&tag))),
                    changes: detail.changes,
                });
            }
            _ => row.left_out = Some(l.label()),
        }
    }
}

/// How many rows the play bar counts as "not on this build": every mod the build left out,
/// whatever the reason.
pub(super) fn not_on_build(rows: &[Row]) -> usize {
    rows.iter()
        .filter(|r| r.unfit.is_some() || r.left_out.is_some())
        .count()
}

impl Row {
    pub(super) fn native(&self) -> bool {
        self.make == Some(Make::Native)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Group {
    NeedsYou,
    Updates,
    Rest,
}

impl Row {
    pub(super) fn group(&self) -> Group {
        if self.need.is_some() {
            Group::NeedsYou
        } else if self.update.is_some() {
            Group::Updates
        } else {
            Group::Rest
        }
    }
}

/// What `rows` needs to know about the computer, read by the caller.
pub(super) struct World<'a> {
    pub(super) index: Option<&'a Index>,
    pub(super) game: &'a str,
    pub(super) enabled: &'a HashSet<String>,
    /// Native mods whose build isn't made.
    pub(super) unbuilt: &'a HashSet<String>,
    pub(super) native_allowed: bool,
    /// The versions the player picked, by project id.
    pub(super) chosen: &'a std::collections::BTreeMap<String, String>,
    /// Native files the browser didn't install.
    pub(super) local: &'a [Local],
}

/// A native `.smod` in the game folder that the registry browser didn't install (a local file or a
/// GitHub install): where it came from isn't verified, so it is listed and switched, never updated.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Local {
    pub(super) id: String,
    pub(super) version: String,
    pub(super) github: bool,
}

/// The registry mods the browser installed, as rows. Packs, files of other sources and files
/// the browser didn't install are not rows (the caller says how many were left alone).
pub(super) fn rows(entries: &[Entry], w: &World) -> Vec<Row> {
    use sunrise_launcher::browser::{sunrise::mod_id, Kind, SourceId};
    let mut out: Vec<Row> = entries
        .iter()
        .filter(|e| e.source == SourceId::Sunrise && e.kind == Kind::Mod)
        .filter_map(|e| {
            let id = mod_id(e)?.to_owned();
            let p = w.index.and_then(|i| i.project(&e.project));
            let native = sunrise_launcher::native_status::is_native_file(&e.file);
            let make = match (p.and_then(make_of), native) {
                (_, true) => Some(Make::Native),
                (m, false) => m,
            };
            let on = w.enabled.contains(&id);
            let taken_down = w.index.and_then(|i| {
                let hash = e.checksum.strip_prefix("SHA-256:").unwrap_or_default();
                i.revoked(&e.project, hash).map(|r| r.reason.clone())
            });
            let need = if let Some(reason) = taken_down {
                Some(Need::TakenDown(reason))
            } else if native && on && !w.native_allowed {
                Some(Need::Permission)
            } else if native && on && w.unbuilt.contains(&id) {
                Some(Need::Build)
            } else {
                None
            };
            let update = w
                .index
                .zip(p)
                .and_then(|(i, p)| newer(i, p, &e.version, w.game));
            Some(Row {
                id,
                title: e.title.clone(),
                authors: p.map(|p| p.authors.join(", ")).unwrap_or_default(),
                make,
                version: e.number.clone(),
                on,
                need,
                update,
                dependency: e.dependency,
                pinned: w.chosen.contains_key(&e.project),
                local: None,
                icon: p.and_then(|p| p.icon.clone()),
                entry: Some(e.clone()),
                unfit: None,
                left_out: None,
            })
        })
        .collect();
    out.extend(w.local.iter().map(|l| {
        let on = w.enabled.contains(&l.id);
        let need = match on {
            true if !w.native_allowed => Some(Need::Permission),
            true if w.unbuilt.contains(&l.id) => Some(Need::Build),
            _ => None,
        };
        Row {
            id: l.id.clone(),
            title: l.id.clone(),
            authors: String::new(),
            make: Some(Make::Native),
            version: l.version.clone(),
            on,
            need,
            update: None,
            dependency: false,
            pinned: false,
            local: Some(l.clone()),
            icon: None,
            entry: None,
            unfit: None,
            left_out: None,
        }
    }));
    sort(&mut out);
    out
}

/// Rows in the order the list shows them: by group, then by name.
pub(super) fn sort(rows: &mut [Row]) {
    rows.sort_by(|a, b| {
        (a.group(), a.title.to_lowercase()).cmp(&(b.group(), b.title.to_lowercase()))
    });
}

impl PartialOrd for Group {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Group {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

/// The version an update would install: the newest for `game` that wasn't taken down, if it is
/// newer than `installed` (the index lists newest first).
pub(super) fn newer(
    index: &Index,
    p: &index::Project,
    installed: &str,
    game: &str,
) -> Option<String> {
    let target = index.installable(p, game)?;
    let at = |v: &str| p.versions.iter().position(|w| w.version == v);
    (at(&target.version)? < at(installed)?).then(|| target.version.clone())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Show {
    All,
    On,
    Off,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RowFilter {
    pub(super) text: String,
    pub(super) show: Show,
    pub(super) makes: BTreeSet<Make>,
}

impl Default for RowFilter {
    fn default() -> Self {
        Self {
            text: String::new(),
            show: Show::All,
            makes: BTreeSet::new(),
        }
    }
}

/// Indexes into `rows` that pass the filter; the order of `rows` (grouped, by name) is kept.
pub(super) fn visible(rows: &[Row], f: &RowFilter) -> Vec<usize> {
    let words: Vec<String> = f.text.split_whitespace().map(str::to_lowercase).collect();
    (0..rows.len())
        .filter(|&i| {
            let r = &rows[i];
            let hay = format!("{} {} {}", r.id, r.title, r.authors).to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
                && match f.show {
                    Show::All => true,
                    Show::On => r.on,
                    Show::Off => !r.on,
                }
                && (f.makes.is_empty() || r.make.is_some_and(|m| f.makes.contains(&m)))
        })
        .collect()
}

// ---------------------------------------------------------------- batches

/// What "Update N mods" would do, shown before anything is downloaded.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct UpdatePlan {
    pub(super) included: Vec<Included>,
    pub(super) skipped: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Included {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) from: String,
    pub(super) to: String,
    pub(super) native: bool,
}

/// The whole pack's compatible updates, not only the visible rows. Native code is only updated
/// with the player's permission, and a version the player chose stays. A mod taken down is
/// updated: the new file is checked like any install and replaces the one that was taken down.
pub(super) fn plan_updates(rows: &[Row], native_allowed: bool) -> UpdatePlan {
    let mut plan = UpdatePlan::default();
    for r in rows.iter().filter(|r| r.local.is_none()) {
        let Some(to) = &r.update else { continue };
        if !r.on {
            // An update never switches a mod on, even for a moment: a mod that is off waits.
            plan.skipped.push((
                r.title.clone(),
                tr("turned off: turn it on first, then update").to_string(),
            ));
        } else if r.pinned {
            plan.skipped.push((
                r.title.clone(),
                tr("you chose its version: update it on its own").to_string(),
            ));
        } else if r.native() && !native_allowed {
            plan.skipped.push((
                r.title.clone(),
                tr("native code: allow it first, then update this mod on its own").to_string(),
            ));
        } else {
            plan.included.push(Included {
                id: r.id.clone(),
                title: r.title.clone(),
                from: r.version.clone(),
                to: to.clone(),
                native: r.native(),
            });
        }
    }
    plan
}

/// Which of the chosen mods can be turned on now, and which can't (and why).
pub(super) fn turn_on_plan<'a>(
    chosen: &[&'a Row],
    native_allowed: bool,
) -> (Vec<&'a Row>, Vec<(&'a Row, String)>) {
    let (mut ok, mut no) = (Vec::new(), Vec::new());
    for r in chosen {
        match &r.need {
            Some(Need::TakenDown(why)) => {
                no.push((*r, trf("taken down: {reason}", &[("reason", why)])))
            }
            _ if r.native() && !native_allowed => no.push((
                *r,
                tr("native code needs your permission first").to_string(),
            )),
            _ => ok.push(*r),
        }
    }
    (ok, no)
}

/// Installed mods that are not being removed but whose newest registry version lists a removed
/// mod among what it needs: "(name, [needed names])".
pub(super) fn dependents(
    rows: &[Row],
    removing: &HashSet<String>,
    index: Option<&Index>,
) -> Vec<(String, Vec<String>)> {
    let Some(index) = index else {
        return Vec::new();
    };
    rows.iter()
        .filter(|r| !removing.contains(&r.id))
        .filter_map(|r| {
            let e = r.entry.as_ref()?;
            let p = index.project(&e.project)?;
            let v = p.versions.iter().find(|v| v.version == e.version)?;
            let needed: Vec<String> = v
                .requires
                .iter()
                .filter(|q| removing.contains(&q.id))
                .map(|q| {
                    index
                        .project(&q.id)
                        .map_or(q.id.clone(), |m| m.name.clone())
                })
                .collect();
            (!needed.is_empty()).then(|| (r.title.clone(), needed))
        })
        .collect()
}

#[cfg(test)]
mod tests;
