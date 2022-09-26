use std::io;
use std::{io::Write, path::PathBuf};

use crate::drone::{wallet_platform_system_status, DroneBuildInfo, DroneStatus};
use ::csv::WriterBuilder;
use serde::Serialize;
use std::collections::HashMap;
use url::Url;

// Report should include                                                                                                                                                                                   (Await-finish - Drone2-start)
// PR_Number | PR_URL| Git_Sha | Drone1_Build_Number | Drone2_Build_Number | Drone1_Unit_Test_Status | Drone1_Await_Test_Status | Drone2_Notify_Test_Status | Drone1_Unit_Test_Elapsed_Time | Drone2_System_Elapsed_Time + Await_Status_Complete | Await_Within_Three_Minutes_Of_Unit_Test_Start | Delta_Await_Status_Finished_To_Drone1_Unit_Test_Start
//    u32     String    String           u32                     u32                DroneStatus                DroneStatus                 DroneStatus                      u32 (sec)                              u32 (sec)                                                         bool                                        u32 (sec)

#[derive(Debug, Serialize)]
pub struct Row {
    pub pr_number: String,
    pub pr_url: Url,
    pub git_sha: String,
    pub drone1_build_number: u32,
    pub drone2_build_number: u32,
    pub drone1_unit_test_status: DroneStatus,
    pub drone1_await_test_status: DroneStatus,
    pub drone2_system_status: DroneStatus,
    pub drone1_unit_test_elapsed_time: i64,
    pub drone2_total_elapsed_time: i64,
    pub await_within_three_minutes_of_unit_test_start: bool,
    pub delta_await_complete_to_unit_test_start: i64,
}

pub fn write_csv(
    commit_build_map: HashMap<String, (Vec<DroneBuildInfo>, Vec<DroneBuildInfo>)>,
    output: Option<PathBuf>,
) {
    if let Some(file_name) = output {
        write_csv_aux(
            commit_build_map,
            WriterBuilder::new()
                .delimiter(b'\t')
                .from_path(file_name)
                .unwrap(),
        );
    } else {
        write_csv_aux(
            commit_build_map,
            WriterBuilder::new()
                .delimiter(b'\t')
                .from_writer(io::stdout().lock()),
        );
    }
}

fn write_csv_aux<W: Write>(
    commit_build_map: HashMap<String, (Vec<DroneBuildInfo>, Vec<DroneBuildInfo>)>,
    mut csv_writer: csv::Writer<W>,
) {
    for (git_sha, (mut drone1_builds, mut drone2_builds)) in commit_build_map {
        // if there aren't builds to compare, continue
        if drone1_builds.is_empty() || drone2_builds.is_empty() {
            continue;
        }

        // order builds by build number
        drone1_builds.sort_by(|a, b| a.build_info.number.cmp(&b.build_info.number));
        drone2_builds.sort_by(|a, b| a.build_info.number.cmp(&b.build_info.number));

        let drone1_build = &drone1_builds[0];
        let drone2_build = &drone2_builds[0];

        let pr_number = drone1_build.get_pr_number();
        let pr_url = drone2_build.get_pr_url();
        let drone1_build_number = drone1_build.build_info.number;
        let drone2_build_number = drone2_build.build_info.number;
        let drone1_stage = drone1_build.get_stage("build-pull-request");
        let drone1_stage = match drone1_stage {
            Some(stage) => stage,
            None => {
                println!("No stage 'build-pull-request' in build '{drone1_build_number}'");
                continue;
            }
        };
        let drone1_unit_test_step = match drone1_stage.get_step("run-wallet-platform-unit-tests") {
            Some(step) => step,
            None => {
                println!(
                    "No step 'run-wallet-platform-unit-tests' in build '{drone1_build_number}'"
                );
                continue;
            }
        };
        if drone1_unit_test_step.get_status() == DroneStatus::Skipped {
            continue;
        }
        let drone1_await_test_step =
            match drone1_stage.get_step("await-wallet-platform-test-status") {
                Some(step) => step,
                None => {
                    println!(
                    "No step 'await-wallet-platform-test-status' in build '{drone1_build_number}'"
                );
                    continue;
                }
            };

        let drone1_unit_test_status = drone1_unit_test_step.get_status();
        let drone1_await_test_status = drone1_await_test_step.get_status();
        let drone2_system_status = wallet_platform_system_status(drone2_build);

        let drone1_unit_test_elapsed_time = drone1_unit_test_step.elapsed_time();
        let drone2_total_elapsed_time = drone1_await_test_step.get_stopped_timestamp()
            - drone2_build.build_info.timestamps.started;
        let delta_await_complete_to_unit_test_start = drone1_await_test_step
            .get_stopped_timestamp()
            - drone1_unit_test_step.get_started_timestamp();
        let await_within_three_minutes_of_unit_test_start =
            delta_await_complete_to_unit_test_start < 60 * 3;

        let record = Row {
            pr_number,
            pr_url,
            git_sha,
            drone1_build_number,
            drone2_build_number,
            drone1_unit_test_status,
            drone1_await_test_status,
            drone2_system_status,
            drone1_unit_test_elapsed_time,
            drone2_total_elapsed_time,
            await_within_three_minutes_of_unit_test_start,
            delta_await_complete_to_unit_test_start,
        };
        csv_writer.serialize(record).unwrap();
    }
}
