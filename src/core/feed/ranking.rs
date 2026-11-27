use super::*;

const DAY_SECS: i64 = 86_400;
const MAX_CANDIDATES: usize = 1_500;
const RANKING_CANDIDATES: usize = 500;

#[derive(Debug, Clone)]
struct Candidate {
    video: Video,
    source: String,
    fetched_at: i64,
    title_affinity: i64,
}

#[derive(Debug, Default)]
struct Affinity {
    channels: HashMap<String, i64>,
    title_tokens: HashMap<String, i64>,
}

struct RankContext {
    subscribed_channels: HashSet<String>,
    affinity: Affinity,
    max_channel_affinity: i64,
    co_watch: HashMap<String, i64>,
    excluded_videos: HashSet<String>,
    excluded_channels: HashSet<String>,
}

struct ScoredCandidate {
    score: i64,
    candidate: Candidate,
}

fn map_candidate(row: &rusqlite::Row<'_>) -> rusqlite::Result<Candidate> {
    Ok(Candidate {
        video: map_video(row)?,
        source: row.get(7)?,
        fetched_at: row.get(8)?,
        title_affinity: -1,
    })
}

fn title_tokens(title: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in title.chars() {
        if ch.is_alphanumeric() {
            current.extend(ch.to_lowercase());
        } else if !current.is_empty() {
            if current.len() > 2 {
                tokens.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
    }

    if current.len() > 2 {
        tokens.push(current);
    }

    tokens.sort();
    tokens.dedup();
    tokens
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 14_695_981_039_346_656_037_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    hash
}

fn load_affinity(conn: &Connection, now: i64) -> Result<Affinity> {
    let mut stmt = conn.prepare(
        "SELECT h.title, h.channel_id, h.last_watched,
                COALESCE(s.total_watched, 0), COALESCE(s.completed_count, 0)
         FROM history_view h
         LEFT JOIN watch_stats s ON s.video_id = h.video_id
         ORDER BY h.last_watched DESC
         LIMIT 1000",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, f64>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;

    let mut affinity = Affinity::default();
    for row in rows {
        let (title, channel_id, last_watched, total_watched, completed_count) = row?;
        let age = (now - last_watched).max(0);
        let recency_weight = (5 - age / (2 * DAY_SECS)).clamp(1, 5);
        let watch_minutes = (total_watched / 60.0).max(0.0);
        let watch_quality = (watch_minutes.sqrt() as i64).clamp(0, 4);
        let completion_quality = completed_count.clamp(0, 3);
        let weight = (recency_weight + watch_quality + completion_quality).clamp(1, 12);

        if let Some(channel_id) = channel_id {
            *affinity.channels.entry(channel_id).or_insert(0) += weight;
        }

        for token in title_tokens(&title) {
            *affinity.title_tokens.entry(token).or_insert(0) += weight;
        }
    }

    Ok(affinity)
}

fn load_rank_context(conn: &Connection, now: i64) -> Result<RankContext> {
    let subscribed_channels = db::channel_list(conn)?
        .into_iter()
        .map(|channel| channel.id)
        .collect();
    let affinity = load_affinity(conn, now)?;
    let max_channel_affinity = affinity.channels.values().copied().max().unwrap_or(0);
    let co_watch = db::co_watch_counts(conn, 100)?;

    let mut excluded_videos = HashSet::new();
    let mut excluded_channels = HashSet::new();
    for target in db::feedback_targets(conn)? {
        match target {
            FeedbackTarget::Video(video_id) => {
                excluded_videos.insert(video_id);
            }
            FeedbackTarget::Channel(channel_id) => {
                excluded_channels.insert(channel_id);
            }
        }
    }

    Ok(RankContext {
        subscribed_channels,
        affinity,
        max_channel_affinity,
        co_watch,
        excluded_videos,
        excluded_channels,
    })
}

fn load_feed_candidates(conn: &Connection, include_watched: bool) -> Result<Vec<Candidate>> {
    let mut stmt = conn.prepare(
        "SELECT f.video_id, f.title, f.channel_id, f.channel_name, f.duration,
                f.thumbnail_url, f.published_at, f.source, f.fetched_at
         FROM feed_cache_view f
         WHERE ?1 = 1 OR NOT EXISTS (
             SELECT 1 FROM history_view h WHERE h.video_id = f.video_id
         )
         ORDER BY COALESCE(f.published_at, 0) DESC, f.fetched_at DESC, f.video_id
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(
        params![include_watched, MAX_CANDIDATES as i64],
        map_candidate,
    )?;
    let mut candidates = rows.collect::<Result<Vec<_>, _>>()?;

    if include_watched {
        let mut history_stmt = conn.prepare(
            "SELECT video_id, title, channel_id, channel_name, duration,
                    thumbnail_url, NULL AS published_at, 'history', last_watched
             FROM history_view
             ORDER BY last_watched DESC
             LIMIT ?1",
        )?;
        let history_rows = history_stmt.query_map(params![MAX_CANDIDATES as i64], map_candidate)?;
        let known_ids: HashSet<String> = candidates
            .iter()
            .map(|candidate| candidate.video.id.clone())
            .collect();
        for candidate in history_rows {
            let candidate = candidate?;
            if !known_ids.contains(&candidate.video.id) {
                candidates.push(candidate);
            }
        }
    }

    Ok(candidates)
}

fn is_eligible(candidate: &Candidate, context: &RankContext) -> bool {
    if context.excluded_videos.contains(&candidate.video.id) {
        return false;
    }

    !candidate
        .video
        .channel_id
        .as_deref()
        .is_some_and(|channel_id| context.excluded_channels.contains(channel_id))
}

fn channel_affinity_score(candidate: &Candidate, context: &RankContext) -> i64 {
    if context.max_channel_affinity == 0 {
        return 0;
    }

    let Some(channel_id) = candidate.video.channel_id.as_deref() else {
        return 0;
    };
    let value = context
        .affinity
        .channels
        .get(channel_id)
        .copied()
        .unwrap_or(0);
    (value * 14 / context.max_channel_affinity).clamp(0, 14)
}

fn title_affinity_score(candidate: &Candidate, context: &RankContext) -> i64 {
    if candidate.title_affinity >= 0 {
        return candidate.title_affinity;
    }
    let tokens = title_tokens(&candidate.video.title);
    if tokens.is_empty() {
        return 0;
    }

    let overlap: i64 = tokens
        .iter()
        .filter_map(|token| context.affinity.title_tokens.get(token))
        .sum();
    (overlap * 18 / (tokens.len() as i64 * 5)).clamp(0, 18)
}

fn recency_score(published_at: Option<i64>, now: i64) -> i64 {
    let Some(published_at) = published_at else {
        return 0;
    };
    let age = (now - published_at).max(0);
    match age {
        0..=DAY_SECS => 12,
        _ if age <= 7 * DAY_SECS => 8,
        _ if age <= 30 * DAY_SECS => 5,
        _ if age <= 90 * DAY_SECS => 2,
        _ => 0,
    }
}

fn freshness_score(fetched_at: i64, now: i64) -> i64 {
    let age = (now - fetched_at).max(0);
    match age {
        0..=3_600 => 6,
        _ if age <= DAY_SECS => 4,
        _ if age <= 7 * DAY_SECS => 2,
        _ => 0,
    }
}

fn source_score(source: &str) -> i64 {
    match source {
        "discover" => 5,
        "subscription" => 4,
        "search" => 3,
        "channel" => 2,
        _ => 1,
    }
}

fn exploration_score(video_id: &str) -> i64 {
    (stable_hash(video_id) % 4) as i64
}

fn personalized_score(candidate: &Candidate, context: &RankContext, now: i64) -> i64 {
    let subscription_score = if candidate
        .video
        .channel_id
        .as_deref()
        .is_some_and(|channel_id| context.subscribed_channels.contains(channel_id))
    {
        28
    } else {
        0
    };
    let channel_score = channel_affinity_score(candidate, context);
    let title_score = title_affinity_score(candidate, context);
    let co_watch_score = context
        .co_watch
        .get(&candidate.video.id)
        .copied()
        .unwrap_or(0)
        .saturating_mul(12)
        .min(12);
    let affinity_score =
        (subscription_score + channel_score + title_score + co_watch_score).min(72);

    affinity_score
        + recency_score(candidate.video.published_at, now)
        + freshness_score(candidate.fetched_at, now)
        + source_score(&candidate.source)
        + exploration_score(&candidate.video.id)
}

fn score_cmp(left: &ScoredCandidate, right: &ScoredCandidate) -> Ordering {
    right
        .score
        .cmp(&left.score)
        .then_with(|| {
            right
                .candidate
                .video
                .published_at
                .cmp(&left.candidate.video.published_at)
        })
        .then_with(|| right.candidate.fetched_at.cmp(&left.candidate.fetched_at))
        .then_with(|| left.candidate.video.id.cmp(&right.candidate.video.id))
}

fn recall_candidates(
    candidates: Vec<Candidate>,
    context: &RankContext,
    now: i64,
) -> Vec<Candidate> {
    let mut recalled = candidates
        .into_iter()
        .filter(|candidate| is_eligible(candidate, context))
        .map(|mut candidate| {
            let title_score = title_affinity_score(&candidate, context);
            candidate.title_affinity = title_score;
            let subscription_score = if candidate
                .video
                .channel_id
                .as_deref()
                .is_some_and(|channel_id| context.subscribed_channels.contains(channel_id))
            {
                28
            } else {
                0
            };
            let score = subscription_score
                + channel_affinity_score(&candidate, context)
                + title_affinity_score(&candidate, context)
                + source_score(&candidate.source)
                + recency_score(candidate.video.published_at, now)
                + freshness_score(candidate.fetched_at, now)
                + exploration_score(&candidate.video.id);
            (score, candidate)
        })
        .collect::<Vec<_>>();
    recalled.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.video.published_at.cmp(&left.1.video.published_at))
            .then_with(|| right.1.fetched_at.cmp(&left.1.fetched_at))
            .then_with(|| left.1.video.id.cmp(&right.1.video.id))
    });
    recalled
        .into_iter()
        .take(RANKING_CANDIDATES)
        .map(|(_, candidate)| candidate)
        .collect()
}

fn rank_candidates(
    candidates: Vec<Candidate>,
    context: &RankContext,
    now: i64,
) -> Vec<ScoredCandidate> {
    let mut scored: Vec<ScoredCandidate> = candidates
        .into_iter()
        .filter(|candidate| is_eligible(candidate, context))
        .map(|candidate| {
            let score = personalized_score(&candidate, context, now);
            ScoredCandidate { score, candidate }
        })
        .collect();
    scored.sort_by(score_cmp);
    scored
}

fn channel_key(video: &Video) -> String {
    video.channel_id.clone().unwrap_or_default()
}

fn select_diverse(scored: &[ScoredCandidate], limit: usize) -> Vec<Video> {
    if limit == 0 || scored.is_empty() {
        return Vec::new();
    }

    let unique_channels: HashSet<String> = scored
        .iter()
        .map(|item| channel_key(&item.candidate.video))
        .collect();
    if unique_channels.len() <= 1 {
        return scored
            .iter()
            .take(limit)
            .map(|item| item.candidate.video.clone())
            .collect();
    }

    let caps = [limit.div_ceil(3).max(1), limit.div_ceil(2).max(2), limit];
    let mut selected = vec![false; scored.len()];
    let mut channel_counts: HashMap<String, usize> = HashMap::new();
    let mut result = Vec::with_capacity(limit);
    let mut last_channel: Option<String> = None;

    for cap in caps {
        while result.len() < limit {
            let eligible = |index: usize| {
                !selected[index]
                    && channel_counts
                        .get(&channel_key(&scored[index].candidate.video))
                        .copied()
                        .unwrap_or(0)
                        < cap
            };
            let different_channel = (0..scored.len()).find(|index| {
                eligible(*index)
                    && channel_key(&scored[*index].candidate.video)
                        != last_channel.clone().unwrap_or_default()
            });
            let index =
                different_channel.or_else(|| (0..scored.len()).find(|index| eligible(*index)));
            let Some(index) = index else {
                break;
            };

            selected[index] = true;
            let channel = channel_key(&scored[index].candidate.video);
            *channel_counts.entry(channel.clone()).or_insert(0) += 1;
            last_channel = Some(channel);
            result.push(scored[index].candidate.video.clone());
        }
    }

    result
}

fn select_with_exploration(scored: &[ScoredCandidate], limit: usize, seed: u64) -> Vec<Video> {
    let exploration_count = if limit > 1 {
        (limit / 5).clamp(1, 6)
    } else {
        0
    };
    if exploration_count == 0 {
        return select_diverse(scored, limit);
    }

    let personalized_limit = limit - exploration_count;
    let mut result = select_diverse(scored, personalized_limit);
    let mut selected_ids: HashSet<String> = result.iter().map(|video| video.id.clone()).collect();
    let mut channel_counts: HashMap<String, usize> = HashMap::new();
    for video in &result {
        *channel_counts.entry(channel_key(video)).or_insert(0) += 1;
    }

    let mut shuffled: Vec<&ScoredCandidate> = scored.iter().collect();
    let mut rng = StdRng::seed_from_u64(seed);
    shuffled.shuffle(&mut rng);

    let channel_cap = (limit / 3).max(1);
    let mut random_videos = Vec::with_capacity(exploration_count);
    for item in &shuffled {
        if random_videos.len() == exploration_count {
            break;
        }
        if selected_ids.contains(&item.candidate.video.id) {
            continue;
        }
        let channel = channel_key(&item.candidate.video);
        if channel_counts.get(&channel).copied().unwrap_or(0) >= channel_cap {
            continue;
        }
        selected_ids.insert(item.candidate.video.id.clone());
        *channel_counts.entry(channel).or_insert(0) += 1;
        random_videos.push(item.candidate.video.clone());
    }

    if random_videos.len() < exploration_count {
        for item in &shuffled {
            if random_videos.len() == exploration_count {
                break;
            }
            if selected_ids.contains(&item.candidate.video.id) {
                continue;
            }
            selected_ids.insert(item.candidate.video.id.clone());
            random_videos.push(item.candidate.video.clone());
        }
    }

    result.extend(random_videos);
    result
}

pub(super) fn ranked_feed_at(conn: &Connection, limit: usize, now: i64) -> Result<Vec<Video>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let context = load_rank_context(conn, now)?;
    let candidates = recall_candidates(load_feed_candidates(conn, false)?, &context, now);
    let scored = rank_candidates(candidates, &context, now);
    Ok(select_with_exploration(&scored, limit, now as u64))
}

pub(super) fn ranked_subscriptions_at(
    conn: &Connection,
    limit: usize,
    now: i64,
) -> Result<Vec<Video>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let context = load_rank_context(conn, now)?;
    let candidates = recall_candidates(
        load_feed_candidates(conn, false)?
            .into_iter()
            .filter(|candidate| {
                candidate
                    .video
                    .channel_id
                    .as_deref()
                    .is_some_and(|channel_id| context.subscribed_channels.contains(channel_id))
            })
            .collect(),
        &context,
        now,
    );
    let scored = rank_candidates(candidates, &context, now);
    Ok(select_diverse(&scored, limit))
}

fn discover_candidates(candidates: Vec<Candidate>) -> Vec<Candidate> {
    candidates
        .into_iter()
        .filter(|candidate| {
            candidate.source == "discover"
                || candidate.video.channel_id.as_deref() == Some("DISCOVER")
        })
        .collect()
}

pub(super) fn ranked_discover_at(conn: &Connection, limit: usize, now: i64) -> Result<Vec<Video>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let context = load_rank_context(conn, now)?;
    let candidates = recall_candidates(
        discover_candidates(load_feed_candidates(conn, false)?),
        &context,
        now,
    );
    let scored = rank_candidates(candidates, &context, now);
    Ok(select_diverse(&scored, limit))
}

fn up_next_score(candidate: &Candidate, current: &Video, context: &RankContext, now: i64) -> i64 {
    let current_tokens: HashSet<String> = title_tokens(&current.title).into_iter().collect();
    let overlap = title_tokens(&candidate.video.title)
        .into_iter()
        .filter(|token| current_tokens.contains(token))
        .count() as i64;

    let same_channel = candidate
        .video
        .channel_id
        .as_deref()
        .is_some_and(|channel_id| current.channel_id.as_deref() == Some(channel_id));
    let duration_score = match (candidate.video.duration, current.duration) {
        (Some(candidate), Some(current)) if candidate.abs_diff(current) <= 120 => 8,
        (Some(candidate), Some(current)) if candidate.abs_diff(current) <= 600 => 4,
        _ => 0,
    };

    let title_score = title_affinity_score(candidate, context);
    let channel_score = channel_affinity_score(candidate, context);
    let co_watch_score = context
        .co_watch
        .get(&candidate.video.id)
        .copied()
        .unwrap_or(0)
        .min(12);

    overlap * 12
        + if same_channel { 10 } else { 0 }
        + duration_score
        + title_score
        + channel_score
        + co_watch_score
        + recency_score(candidate.video.published_at, now)
        + freshness_score(candidate.fetched_at, now)
        + source_score(&candidate.source)
        + exploration_score(&candidate.video.id)
}

pub(super) fn ranked_up_next_at(
    conn: &Connection,
    current: &Video,
    limit: usize,
    now: i64,
) -> Result<Vec<Video>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let context = load_rank_context(conn, now)?;
    let candidates: Vec<Candidate> = recall_candidates(
        load_feed_candidates(conn, true)?
            .into_iter()
            .filter(|candidate| candidate.video.id != current.id)
            .collect(),
        &context,
        now,
    );
    let mut scored: Vec<ScoredCandidate> = candidates
        .into_iter()
        .map(|candidate| {
            let score = up_next_score(&candidate, current, &context, now);
            ScoredCandidate { score, candidate }
        })
        .collect();
    scored.sort_by(score_cmp);
    Ok(select_diverse(&scored, limit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_stage_caps_candidates_before_full_scoring() {
        let now = 1_000;
        let context = RankContext {
            subscribed_channels: HashSet::new(),
            affinity: Affinity::default(),
            max_channel_affinity: 0,
            co_watch: HashMap::new(),
            excluded_videos: HashSet::new(),
            excluded_channels: HashSet::new(),
        };
        let candidates = (0..700)
            .map(|index| Candidate {
                video: Video {
                    id: format!("video-{index}"),
                    title: format!("Video {index}"),
                    published_at: Some(now - index),
                    ..Video::default()
                },
                source: "cache".into(),
                fetched_at: now - index,
                title_affinity: -1,
            })
            .collect();
        let recalled = recall_candidates(candidates, &context, now);
        assert_eq!(recalled.len(), RANKING_CANDIDATES);
    }
}
