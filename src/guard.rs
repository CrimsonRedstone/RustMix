//! S/PDIF in reaching S/PDIF out is a loop: the same pedal can sit on both jacks.
//! The patch is unity and the pedal's gain is not in this unit, so there is no
//! percent and no consent. Monitor level and headphone level are never read.
//! A bus carries a channel only when that channel's fader is above silence and
//! the channel is not muted. Solo and the bus master do not clear the path.
//! The computer is not part of the path. An existing loop is named and left.
//! Disconnect stays allowed.

use crate::topology::{bus_name, input_name, classify, SourceKind, BUSES, INPUTS, NOTHING};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Node {
    Bus(u16),
    In(u16),
    Out(u16),
    Src(u16),
}

#[derive(Clone, Copy, Debug)]
pub struct View<'a> {
    pub patch: &'a [u8],
    pub faders: Option<&'a [f32]>,
    pub mutes: Option<&'a [u8]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verdict {
    pub ok: bool,
    pub hint: String,
}

pub fn check(before: View<'_>, after: View<'_>) -> Verdict {
    let graph = Graph::new(after);
    let after_edges = graph.loop_edges();
    if !after_edges.is_empty() {
        let before_ids: Vec<(Node, Node)> = Graph::new(before).loop_edges();
        let added: Vec<(Node, Node)> = after_edges
            .iter()
            .copied()
            .filter(|edge| !before_ids.contains(edge))
            .collect();
        let raised: Vec<(Node, Node)> = raised_edges(before, after)
            .into_iter()
            .filter(|edge| after_edges.contains(edge))
            .collect();
        if let Some(edge) = added.first().or(raised.first()) {
            let hops = graph.hops_through(*edge);
            return Verdict { ok: false, hint: phrase(&hops, false) };
        }
    }
    if blind_bus_route(before, after) {
        return Verdict {
            ok: false,
            hint: "The mix is not loaded yet, so that route to S/PDIF out stays off.".into(),
        };
    }
    Verdict { ok: true, hint: String::new() }
}

pub fn judge(state: View<'_>, jobs: &[(u16, u8)]) -> Verdict {
    let mut patch = state.patch.to_vec();
    for &(slot, source) in jobs {
        if (slot as usize) < patch.len() && source <= NOTHING {
            patch[slot as usize] = source;
        }
    }
    check(state, View { patch: &patch, faders: state.faders, mutes: state.mutes })
}

pub fn present(state: View<'_>) -> String {
    let graph = Graph::new(state);
    let edges = graph.loop_edges();
    if edges.is_empty() {
        String::new()
    } else {
        phrase(&graph.hops_through(edges[0]), true)
    }
}

/// Preview a mix-cell edit. `gain` is linear. `muted` replaces the mute bit when set.
pub fn preview_mix(
    state: View<'_>,
    index: usize,
    gain: Option<f32>,
    muted: Option<bool>,
) -> Verdict {
    let Some(faders) = state.faders else {
        return Verdict { ok: true, hint: String::new() };
    };
    if index >= faders.len() {
        return Verdict { ok: true, hint: String::new() };
    }
    let mut next_faders = faders.to_vec();
    let mut next_mutes = state.mutes.unwrap_or(&[]).to_vec();
    if next_mutes.len() < next_faders.len() {
        next_mutes.resize(next_faders.len(), 0);
    }
    if let Some(gain) = gain {
        let old = if faders[index] > 0.0 { faders[index] } else { 0.0 };
        if muted.is_none() && !(gain > old) {
            return Verdict { ok: true, hint: String::new() };
        }
        next_faders[index] = gain;
    }
    if let Some(muted) = muted {
        if gain.is_none() {
            let was = next_mutes[index] != 0;
            if muted || !was {
                return Verdict { ok: true, hint: String::new() };
            }
        }
        next_mutes[index] = if muted { 1 } else { 0 };
    }
    check(
        state,
        View { patch: state.patch, faders: Some(&next_faders), mutes: Some(&next_mutes) },
    )
}

fn phrase(hops: &[String], already: bool) -> String {
    if hops.len() < 2 {
        return String::new();
    }
    let mid = &hops[1..hops.len() - 1];
    let mut text = format!(
        "{} {} {}",
        hops[0],
        if already { "already reaches" } else { "reaches" },
        hops[hops.len() - 1]
    );
    if !mid.is_empty() {
        text.push_str(" through ");
        text.push_str(&mid.join(", "));
    }
    text.push_str(if already { ". Nothing was changed." } else { ". That route stays off." });
    text
}

fn faders_missing(state: View<'_>) -> bool {
    state.faders.map(|faders| faders.is_empty()).unwrap_or(true)
}

fn source_at(patch: &[u8], slot: u16) -> Option<u8> {
    let value = *patch.get(slot as usize)?;
    if value < NOTHING { Some(value) } else { None }
}

fn carries(state: View<'_>, bus: u16, input: u16) -> bool {
    let Some(faders) = state.faders else { return false };
    if faders.is_empty() {
        return false;
    }
    let index = bus as usize * INPUTS + input as usize;
    let Some(gain) = faders.get(index).copied() else { return false };
    if !(gain > 0.0) {
        return false;
    }
    let muted = state
        .mutes
        .and_then(|mutes| mutes.get(index).copied())
        .unwrap_or(0);
    muted == 0
}

fn raised_edges(before: View<'_>, after: View<'_>) -> Vec<(Node, Node)> {
    let (Some(old), Some(new)) = (before.faders, after.faders) else {
        return Vec::new();
    };
    let limit = old.len().min(new.len()).min(BUSES * INPUTS);
    let mut edges = Vec::new();
    for index in 0..limit {
        let old_lin = if old[index] > 0.0 { old[index] } else { 0.0 };
        let new_lin = if new[index].is_finite() { new[index] } else { 0.0 };
        if new_lin > old_lin {
            let bus = (index / INPUTS) as u16;
            let input = (index % INPUTS) as u16;
            edges.push((Node::In(input), Node::Bus(bus)));
        }
    }
    edges.sort_by_key(|edge| edge_key(*edge));
    edges
}

fn blind_bus_route(before: View<'_>, after: View<'_>) -> bool {
    if !faders_missing(before) && !faders_missing(after) {
        return false;
    }
    for slot in 58..=59 {
        let next = source_at(after.patch, slot);
        let prev = source_at(before.patch, slot);
        if next.is_some() && next != prev && (76..=91).contains(&next.unwrap()) {
            return true;
        }
    }
    false
}

fn edge_key(edge: (Node, Node)) -> (Node, Node) {
    edge
}

struct Graph<'a> {
    state: View<'a>,
    fed: Vec<(u8, Vec<Node>)>,
    post_fx: Vec<(u16, Vec<u16>)>,
}

impl<'a> Graph<'a> {
    fn new(state: View<'a>) -> Self {
        let mut fed: Vec<(u8, Vec<Node>)> = Vec::new();
        let mut add = |slot: u16, node: Node| {
            let Some(code) = source_at(state.patch, slot) else { return };
            if let Some((_, nodes)) = fed.iter_mut().find(|(have, _)| *have == code) {
                nodes.push(node);
            } else {
                fed.push((code, vec![node]));
            }
        };
        for input in 0..INPUTS {
            add(76 + input as u16, Node::In(input as u16));
        }
        add(58, Node::Out(58));
        add(59, Node::Out(59));
        let mut post_fx: Vec<(u16, Vec<u16>)> = Vec::new();
        for (code, _) in &fed {
            let (kind, _) = classify(*code);
            if kind == SourceKind::Post {
                let input = u16::from(code - 92);
                if let Some((_, codes)) = post_fx.iter_mut().find(|(have, _)| *have == input) {
                    codes.push(u16::from(*code));
                } else {
                    post_fx.push((input, vec![u16::from(*code)]));
                }
            }
        }
        Self { state, fed, post_fx }
    }

    fn successors(&self, node: Node) -> Vec<Node> {
        match node {
            Node::Src(code) => self
                .fed
                .iter()
                .find(|(have, _)| *have == code as u8)
                .map(|(_, nodes)| nodes.clone())
                .unwrap_or_default(),
            Node::In(input) => {
                let mut next = Vec::new();
                for bus in 0..BUSES as u16 {
                    if carries(self.state, bus, input) {
                        next.push(Node::Bus(bus));
                    }
                }
                if let Some((_, codes)) = self.post_fx.iter().find(|(have, _)| *have == input) {
                    for code in codes {
                        next.push(Node::Src(*code));
                    }
                }
                next
            }
            Node::Bus(bus) => {
                let codes: &[u16] = if bus == 14 || bus == 15 { &[90, 91] } else { &[76 + bus] };
                codes.iter().copied().map(Node::Src).collect()
            }
            Node::Out(_) => Vec::new(),
        }
    }

    fn search(&self, starts: &[Node], reverse: Option<&[(Node, Vec<Node>)]>) -> Vec<(Node, Option<Node>)> {
        let mut came: Vec<(Node, Option<Node>)> = Vec::new();
        let mut queue = Vec::new();
        for &node in starts {
            if !came.iter().any(|(have, _)| *have == node) {
                came.push((node, None));
                queue.push(node);
            }
        }
        let mut head = 0;
        while head < queue.len() {
            let node = queue[head];
            head += 1;
            let step = if let Some(reverse) = reverse {
                reverse
                    .iter()
                    .find(|(have, _)| *have == node)
                    .map(|(_, nodes)| nodes.clone())
                    .unwrap_or_default()
            } else {
                self.successors(node)
            };
            for next in step {
                if !came.iter().any(|(have, _)| *have == next) {
                    came.push((next, Some(node)));
                    queue.push(next);
                }
            }
        }
        came
    }

    fn reach(&self) -> Reach {
        let forward = self.search(&[Node::Src(58), Node::Src(59)], None);
        let mut reverse: Vec<(Node, Vec<Node>)> = Vec::new();
        for (node, _) in &forward {
            for next in self.successors(*node) {
                if let Some((_, list)) = reverse.iter_mut().find(|(have, _)| *have == next) {
                    list.push(*node);
                } else {
                    reverse.push((next, vec![*node]));
                }
            }
        }
        let mut outs = Vec::new();
        for slot in [Node::Out(58), Node::Out(59)] {
            if forward.iter().any(|(node, _)| *node == slot) {
                outs.push(slot);
            }
        }
        let backward = self.search(&outs, Some(&reverse));
        Reach { forward, backward }
    }

    fn loop_edges(&self) -> Vec<(Node, Node)> {
        let reached = self.reach();
        let mut found = Vec::new();
        for (node, _) in &reached.backward {
            let reverse_of = self.predecessors(&reached, *node);
            for prev in reverse_of {
                let edge = (prev, *node);
                if !found.contains(&edge) {
                    found.push(edge);
                }
            }
        }
        found.sort_by_key(|edge| edge_key(*edge));
        found
    }

    fn predecessors(&self, reached: &Reach, node: Node) -> Vec<Node> {
        let mut list = Vec::new();
        for (from, _) in &reached.forward {
            for next in self.successors(*from) {
                if next == node {
                    list.push(*from);
                }
            }
        }
        list
    }

    fn hops_through(&self, edge: (Node, Node)) -> Vec<String> {
        let reached = self.reach();
        let mut left = walk(&reached.forward, edge.0);
        left.reverse();
        let right = walk(&reached.backward, edge.1);
        let mut nodes = left;
        nodes.extend(right);
        hops_of(&nodes)
    }
}

struct Reach {
    forward: Vec<(Node, Option<Node>)>,
    backward: Vec<(Node, Option<Node>)>,
}

fn walk(came: &[(Node, Option<Node>)], mut node: Node) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut seen = Vec::new();
    loop {
        if seen.contains(&node) {
            break;
        }
        seen.push(node);
        nodes.push(node);
        let Some((_, parent)) = came.iter().find(|(have, _)| *have == node) else { break };
        match parent {
            Some(parent) => node = *parent,
            None => break,
        }
    }
    nodes
}

fn hops_of(nodes: &[Node]) -> Vec<String> {
    let mut hops = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        match *node {
            Node::Src(code) => {
                let (kind, label) = classify(code as u8);
                if index == 0 || kind == SourceKind::Spdif || kind == SourceKind::Post {
                    hops.push(if label.is_empty() { format!("source {code}") } else { label });
                }
            }
            Node::In(n) => hops.push(input_name(n as usize).to_string()),
            Node::Bus(n) => hops.push(bus_name(n as usize).to_string()),
            Node::Out(n) => hops.push(if n == 58 { "S/PDIF out L" } else { "S/PDIF out R" }.into()),
        }
    }
    hops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view<'a>(patch: &'a [u8], faders: Option<&'a [f32]>, mutes: Option<&'a [u8]>) -> View<'a> {
        View { patch, faders, mutes }
    }

    fn patch_of(slots: &[(u16, u8)]) -> Vec<u8> {
        let mut patch = vec![NOTHING; 128];
        for &(slot, source) in slots {
            patch[slot as usize] = source;
        }
        patch
    }

    #[test]
    fn direct_and_crossed_are_named() {
        let before = patch_of(&[]);
        let after = patch_of(&[(58, 58)]);
        let verdict = check(view(&before, None, None), view(&after, None, None));
        assert_eq!(verdict.hint, "S/PDIF in L reaches S/PDIF out L. That route stays off.");
        let crossed = patch_of(&[(59, 58)]);
        let verdict = check(view(&before, None, None), view(&crossed, None, None));
        assert_eq!(verdict.hint, "S/PDIF in L reaches S/PDIF out R. That route stays off.");
    }

    #[test]
    fn through_a_bus_names_the_mix() {
        let before = patch_of(&[]);
        let mut faders = vec![0.0; 16 * 44];
        faders[10] = 1.0;
        let after = patch_of(&[(86, 58), (58, 76)]);
        let verdict = check(view(&before, Some(&faders), None), view(&after, Some(&faders), None));
        assert_eq!(
            verdict.hint,
            "S/PDIF in L reaches S/PDIF out L through S/PDIF L, Main 1. That route stays off."
        );
    }

    #[test]
    fn silent_or_muted_is_not_a_path_and_unmute_is() {
        let patch = patch_of(&[(86, 58), (58, 76)]);
        let mut faders = vec![0.0; 16 * 44];
        let open = check(view(&patch, Some(&faders), None), view(&patch, Some(&faders), None));
        assert!(open.ok);
        faders[10] = 0.8;
        let mut mutes = vec![0u8; faders.len()];
        mutes[10] = 1;
        let held = check(
            view(&patch, Some(&faders), Some(&mutes)),
            view(&patch, Some(&faders), Some(&mutes)),
        );
        assert!(held.ok);
        let verdict = preview_mix(view(&patch, Some(&faders), Some(&mutes)), 10, None, Some(false));
        assert!(!verdict.ok);
        assert!(verdict.hint.contains("stays off"));
    }

    #[test]
    fn reverb_and_post_fx_are_paths_and_the_computer_is_not() {
        let mut faders = vec![0.0; 16 * 44];
        let cell = 14 * 44 + 10;
        faders[cell] = 1.0;
        let before = patch_of(&[]);
        let reverb = patch_of(&[(86, 58), (58, 90)]);
        let verdict = check(
            view(&before, Some(&faders), None),
            view(&reverb, Some(&faders), None),
        );
        assert!(verdict.hint.contains("Reverb 1"), "{}", verdict.hint);
        assert!(!verdict.ok);

        let post = patch_of(&[(76, 58), (58, 92)]);
        let verdict = check(view(&before, Some(&faders), None), view(&post, Some(&faders), None));
        assert!(verdict.hint.contains("post-FX"), "{}", verdict.hint);

        let to_computer = patch_of(&[(12, 58), (13, 59)]);
        assert!(check(view(&before, Some(&faders), None), view(&to_computer, Some(&faders), None)).ok);
        let loopback = patch_of(&[(10, 0), (11, 1)]);
        assert!(check(view(&before, Some(&faders), None), view(&loopback, Some(&faders), None)).ok);
        let mic_to_main = patch_of(&[(37, 47)]);
        assert!(check(view(&before, None, None), view(&mic_to_main, None, None)).ok);
    }

    #[test]
    fn existing_loop_allows_weaken_and_refuses_a_stronger_or_second_path() {
        let mut faders = vec![0.0; 16 * 44];
        faders[10] = 0.5;
        let patch = patch_of(&[(86, 58), (58, 76)]);
        let state = view(&patch, Some(&faders), None);
        assert!(present(state).contains("already reaches"));
        assert!(present(state).contains("Nothing was changed."));
        let mut quieter = faders.clone();
        quieter[10] = 0.2;
        assert!(check(state, view(&patch, Some(&quieter), None)).ok);
        let mut louder = faders.clone();
        louder[10] = 0.9;
        assert!(!check(state, view(&patch, Some(&louder), None)).ok);
        let mut mutes = vec![0u8; faders.len()];
        mutes[10] = 1;
        assert!(check(state, view(&patch, Some(&faders), Some(&mutes))).ok);
        let second = patch_of(&[(86, 58), (58, 76), (59, 59)]);
        assert!(!check(state, view(&second, Some(&faders), None)).ok);
        let cleared = patch_of(&[(86, 58), (58, NOTHING)]);
        assert!(judge(state, &[(58, NOTHING)]).ok);
        assert!(check(state, view(&cleared, Some(&faders), None)).ok);
    }

    #[test]
    fn judge_does_not_mutate_and_blind_mixes_stay_off() {
        let patch = patch_of(&[(37, 76)]);
        let owned = patch.clone();
        let state = view(&patch, None, None);
        let _ = judge(state, &[(58, 58)]);
        assert_eq!(patch, owned);
        let direct = judge(state, &[(58, 58)]);
        assert!(!direct.ok);
        let bus = judge(state, &[(58, 76)]);
        assert_eq!(bus.hint, "The mix is not loaded yet, so that route to S/PDIF out stays off.");
        assert!(judge(state, &[(37, 47)]).ok);
        assert!(judge(state, &[(12, 58)]).ok);
    }
}
