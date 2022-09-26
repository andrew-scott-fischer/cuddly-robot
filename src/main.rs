use clap::Parser;
use drone::{DroneBuildInfo, DroneBuildListItem, DroneClient, DroneEvent, DroneStatus};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod csv;
mod drone;

static BITGO_DRONE1_URL: &str = "https://drone.bitgo-dev.com";
static BITGO_DRONE2_URL: &str = "https://drone2.bitgo-ci.com";

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Window size in hours within which to compare build metrics;
    /// builds must both be created and finished within window
    #[clap(value_parser)]
    window_duration: u64,
    /// Offset in hours to start metric comparison
    #[clap(short, long, value_parser)]
    window_offset: Option<u64>,
    #[clap(short, long, value_parser)]
    file: Option<PathBuf>,
    #[clap(short, long, value_parser)]
    develop: bool,
    #[clap(env = "DRONE1_TOKEN")]
    drone1_token: String,
    #[clap(env = "DRONE2_TOKEN")]
    drone2_token: String,
}

fn timestamp_to_system_time(timestamp: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(timestamp.unsigned_abs())
}

fn get_window_bounds(cli: &Cli) -> (SystemTime, SystemTime) {
    let window_start = if let Some(window_offset) = cli.window_offset {
        SystemTime::now() - Duration::from_secs(window_offset * 60 * 60)
    } else {
        SystemTime::now()
    };
    let window_end = window_start - Duration::from_secs(cli.window_duration * 60 * 60);
    (window_start, window_end)
}
enum FilterState {
    Break,
    Continue,
    DroneBuildInfo(DroneBuildInfo),
}

fn filter_build(
    drone_build_list_item: &DroneBuildListItem,
    window_start: &SystemTime,
    window_end: &SystemTime,
    drone_client: &DroneClient,
    develop: bool,
) -> FilterState {
    // if build was created and finished outside window, unlikely any older builds will be within window, ignore and break
    if timestamp_to_system_time(drone_build_list_item.timestamps.finished) < *window_end
        && timestamp_to_system_time(drone_build_list_item.timestamps.created) < *window_end
    {
        return FilterState::Break;
    }
    // if build was created before window_end or finished after window_start, ignore
    if timestamp_to_system_time(drone_build_list_item.timestamps.finished) > *window_start
        || timestamp_to_system_time(drone_build_list_item.timestamps.created) < *window_end
    {
        return FilterState::Continue;
    }

    if develop {
        if !(drone_build_list_item.event == DroneEvent::Push
            && drone_build_list_item.source == "develop"
            && drone_build_list_item.target == "develop")
        {
            return FilterState::Continue;
        }
    } else {
        if drone_build_list_item.event != DroneEvent::PullRequest {
            return FilterState::Continue;
        }
    }
    if drone_build_list_item.status == DroneStatus::Running {
        return FilterState::Continue;
    }

    FilterState::DroneBuildInfo(drone_client.get_build_info(drone_build_list_item.number))
}

fn drone_build_map(
    window_start: SystemTime,
    window_end: SystemTime,
    drone1_client: DroneClient,
    drone2_client: DroneClient,
    develop: bool,
) -> HashMap<String, (Vec<DroneBuildInfo>, Vec<DroneBuildInfo>)> {
    let mut git_sha_to_builds: HashMap<String, (Vec<DroneBuildInfo>, Vec<DroneBuildInfo>)> =
        HashMap::new();

    for drone_build_list_item in drone1_client.get_builds_paginated() {
        let git_sha_entry = git_sha_to_builds
            .entry(drone_build_list_item.git_metadata.git_sha.clone())
            .or_default();
        match filter_build(
            &drone_build_list_item,
            &window_start,
            &window_end,
            &drone1_client,
            develop,
        ) {
            FilterState::Break => break,
            FilterState::Continue => continue,
            FilterState::DroneBuildInfo(drone_build_info) => git_sha_entry.0.push(drone_build_info),
        }
    }

    for drone_build_list_item in drone2_client.get_builds_paginated() {
        let git_sha_entry = git_sha_to_builds
            .entry(drone_build_list_item.git_metadata.git_sha.clone())
            .or_default();
        match filter_build(
            &drone_build_list_item,
            &window_start,
            &window_end,
            &drone2_client,
            develop
        ) {
            FilterState::Break => break,
            FilterState::Continue => continue,
            FilterState::DroneBuildInfo(drone_build_info) => git_sha_entry.1.push(drone_build_info),
        }
    }
    git_sha_to_builds
}

fn main() {
    let cli = Cli::parse();
    let drone1_client =
        drone::DroneClient::new_with_credentials(BITGO_DRONE1_URL, cli.drone1_token.clone());
    let drone2_client =
        drone::DroneClient::new_with_credentials(BITGO_DRONE2_URL, cli.drone2_token.clone());

    let (window_start, window_end) = get_window_bounds(&cli);

    // window_start and window_end are ordered from the perspective of the start
    // of a drone build list, where builds are in decreasing order from "now"
    // into the past.
    // If this is a list of drone builds, builds compared by this tool
    // would include builds that were created after 'window_end' and builds
    // which finished before 'window_start'; any build is fully contained
    // within the window will be selected for comparison.
    // In the example below, only builds 4568, 4569, and 4570
    // will be selected.
    // (past)-4567---4568---4569---4570---4571---*---*---(now)
    //         ||     ||     ||     ||     ||
    //         vv     ||     vv     ||     ||
    //       |-----|  ||  |-------| ||     ||
    //                vv            vv     ||
    //              |--------| |------|    vv
    //                              |---------|
    //            ^                     ^                   ^
    //            |------- 5 hrs -------|<------ 3 hrs -----|
    //            |   window_duration   |    window_offset  |
    //            |                     |                   |
    //        window_end           window_start

    let commit_sha_to_builds = drone_build_map(
        window_start,
        window_end,
        drone1_client,
        drone2_client,
        cli.develop,
    );

    crate::csv::write_csv(commit_sha_to_builds, cli.file);
}
