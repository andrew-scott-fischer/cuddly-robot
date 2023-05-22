use reqwest::blocking::{Client, ClientBuilder};
use reqwest::header::{HeaderMap, AUTHORIZATION};
use reqwest::Url;
use serde::*;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct DroneClient {
    client: Client,
    url: Url,
}

impl DroneClient {
    pub fn new_with_credentials(url: &'static str, mut credentials: String) -> Self {
        credentials.insert_str(0, "Bearer ");
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, credentials.parse().unwrap());
        let client = ClientBuilder::new()
            .default_headers(headers)
            .build()
            .unwrap();
        DroneClient {
            client,
            url: Url::parse(url).unwrap(),
        }
    }

    fn get_bgms_build_list_with_page(&self, page: usize) -> DroneBuildList {
        let response = self
            .client
            .get(
                self.url
                    .join("/api/repos/BitGo/bitgo-microservices/builds")
                    .unwrap(),
            )
            .query(&[("page", page)])
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .bytes()
            .unwrap();
        serde_json::from_slice(&response).unwrap()
    }

    #[allow(dead_code)]
    pub fn get_recent_builds(&self) -> DroneBuildList {
        self.get_bgms_build_list_with_page(1)
    }

    pub fn get_builds_paginated<'drone>(&'drone self) -> DroneBuildsPaginator<'drone> {
        DroneBuildsPaginator {
            page: 1,
            drone: self,
            cached: DroneBuildList::with_capacity(50),
        }
    }

    pub fn get_build_info(&self, build_number: u32) -> DroneBuildInfo {
        let response = self
            .client
            .get(
                self.url
                    .join("/api/repos/BitGo/bitgo-microservices/builds/")
                    .unwrap()
                    .join(&build_number.to_string())
                    .unwrap(),
            )
            // .get(format!(
            //     "{}/api/repos/BitGo/bitgo-microservices/builds/{build_number}",
            //     self.url
            // ))
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .bytes()
            .unwrap();
        serde_json::from_slice(&response)
            .unwrap_or_else(|error| panic!("Build Number: {build_number} ; Error: {error}"))
    }
}

#[derive(Debug, Clone)]
pub struct DroneBuildsPaginator<'drone> {
    page: usize,
    drone: &'drone DroneClient,
    cached: DroneBuildList,
}

impl DroneBuildsPaginator<'_> {
    #[allow(dead_code)]
    pub fn skip_pages(mut self, pages: usize) -> Self {
        self.skip_pages_mut(pages);
        self
    }

    #[allow(dead_code)]
    pub fn skip_pages_mut(&mut self, pages: usize) -> &mut Self {
        if pages > 0 {
            self.cached.clear();
            self.page += pages;
        }
        self
    }
}

impl Iterator for DroneBuildsPaginator<'_> {
    type Item = DroneBuildListItem;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cached.is_empty() {
            self.cached
                .extend(self.drone.get_bgms_build_list_with_page(self.page));
            self.page += 1;
        }
        self.cached.pop_front()
    }
}

use derive_more::{AsMut, AsRef, Deref, DerefMut, IntoIterator};
use derive_new::new;

#[derive(Debug, Deserialize, Clone, AsRef, AsMut, Deref, DerefMut, IntoIterator, new)]
pub struct DroneBuildList(#[new(default)] VecDeque<DroneBuildListItem>);

impl DroneBuildList {
    fn with_capacity(capacity: usize) -> Self {
        Self(VecDeque::with_capacity(capacity))
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct DroneBuildListItem {
    pub id: u32,
    pub repo_id: u32,
    pub trigger: String,
    pub number: u32,
    pub status: DroneStatus,
    pub event: DroneEvent,
    pub action: String,
    pub link: Url,
    pub timestamp: u64,
    pub message: String,
    #[serde(flatten)]
    pub git_metadata: DroneGitMetadata,
    pub source_repo: String,
    pub source: String,
    pub target: String,
    #[serde(flatten)]
    pub author_data: DroneBuildAuthorData,
    pub sender: String,
    #[serde(flatten)]
    pub timestamps: DroneBuildTimestamps,
    pub version: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DroneGitMetadata {
    #[serde(rename = "before")]
    pub prev_git_sha: String,
    #[serde(rename = "after")]
    pub git_sha: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DroneAction {
    Create,
    Sync,
    #[serde(rename = "")]
    None,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DroneStatus {
    Success,
    Failure,
    Killed,
    Error,
    Running,
    Skipped,
    Pending,
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DroneEvent {
    PullRequest,
    Push,
    Tag,
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DroneBuildTimestamps {
    pub started: i64,
    pub finished: i64,
    pub created: i64,
    pub updated: i64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DroneStageTimestamps {
    pub started: i64,
    pub stopped: i64,
    pub created: i64,
    pub updated: i64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DroneBuildAuthorData {
    pub author_login: String,
    pub author_name: String,
    pub author_email: String,
    pub author_avatar: Url,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DroneBuildInfo {
    #[serde(flatten)]
    pub build_info: DroneBuildListItem,
    pub stages: Vec<DroneStage>,
}

impl DroneBuildInfo {
    pub fn get_pr_url(&self) -> Url {
        self.build_info.link.clone()
    }
    pub fn get_pr_number(&self) -> String {
        self.build_info
            .link
            .path_segments()
            .unwrap()
            .last()
            .unwrap()
            .split('.')
            .next()
            .unwrap()
            .to_string()
    }

    pub fn get_stage(&self, stage_name: &str) -> Option<&DroneStage> {
        self.stages
            .iter()
            .filter(|stage| match stage {
                DroneStage::Drone1Stage(stage) => stage_name == stage.name,
                DroneStage::Drone2Stage(stage) => stage_name == stage.drone_stage.name,
            })
            .next()
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum DroneStage {
    Drone2Stage(Drone2Stage),
    Drone1Stage(Drone1Stage),
}

impl DroneStage {
    pub fn get_step(&self, step_name: &str) -> Option<&DroneStep> {
        let drone_steps: &Vec<DroneStep> = match self {
            DroneStage::Drone1Stage(stage) => &stage.steps,
            DroneStage::Drone2Stage(stage) => &stage.drone_stage.steps,
        };
        drone_steps
            .iter()
            .filter(|step| match step {
                DroneStep::Drone1Step(step) => step.name == step_name,
                DroneStep::Drone2Step(step) => step.drone_step.name == step_name,
            })
            .next()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Drone1Stage {
    pub id: u32,
    pub repo_id: u32,
    pub build_id: u32,
    pub number: u32,
    pub name: String,
    pub status: DroneStatus,
    pub errignore: bool,
    pub exit_code: i32,
    pub machine: Option<String>,
    pub os: String,
    pub arch: String,
    #[serde(flatten)]
    pub timestamps: DroneStageTimestamps,
    pub version: u32,
    pub on_success: bool,
    pub on_failure: bool,
    #[serde(default)]
    pub steps: Vec<DroneStep>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Drone2Stage {
    #[serde(flatten)]
    pub drone_stage: Drone1Stage,
    pub kind: String,
    #[serde(rename = "type")]
    pub stage_type: String,
    pub depends_on: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum DroneStep {
    Drone2Step(Drone2Step),
    Drone1Step(Drone1Step),
}

impl DroneStep {
    pub fn get_status(&self) -> DroneStatus {
        match self {
            Self::Drone1Step(step) => step.status,
            Self::Drone2Step(step) => step.drone_step.status,
        }
    }

    pub fn get_started_timestamp(&self) -> i64 {
        match self {
            Self::Drone1Step(step) => step.started,
            Self::Drone2Step(step) => step.drone_step.started,
        }
        .unwrap()
    }

    pub fn get_stopped_timestamp(&self) -> i64 {
        match self {
            Self::Drone1Step(step) => step.stopped,
            Self::Drone2Step(step) => step.drone_step.stopped,
        }
        .unwrap()
    }

    pub fn elapsed_time(&self) -> i64 {
        self.get_stopped_timestamp() - self.get_started_timestamp()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Drone1Step {
    pub id: u32,
    pub step_id: u32,
    pub number: u32,
    pub name: String,
    pub status: DroneStatus,
    pub errignore: Option<bool>,
    pub exit_code: i32,
    pub started: Option<i64>,
    pub stopped: Option<i64>,
    pub version: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Drone2Step {
    #[serde(flatten)]
    pub drone_step: Drone1Step,
    #[serde(default)]
    pub depends_on: Option<Vec<String>>,
    pub image: String,
}

pub fn wallet_platform_system_status(drone_build_info: &DroneBuildInfo) -> DroneStatus {
    if let DroneStage::Drone1Stage(_) = drone_build_info.stages.iter().next().unwrap() {
        panic!("This function only works for drone2 DroneBuildInfos");
    };
    use regex::Regex;
    let re = Regex::new(r"^wallet-platform-.*").unwrap();

    drone_build_info
        .stages
        .iter()
        .map(|stage| {
            if let DroneStage::Drone2Stage(stage) = stage {
                stage
            } else {
                panic!("This function only works for drone2 DroneBuildInfos");
            }
        })
        .filter(|stage| re.is_match(&stage.drone_stage.name))
        .fold(DroneStatus::Success, |status, stage| match status {
            DroneStatus::Failure => DroneStatus::Failure,
            DroneStatus::Success => match stage.drone_stage.status {
                DroneStatus::Success => DroneStatus::Success,
                DroneStatus::Skipped => DroneStatus::Success,
                _ => DroneStatus::Failure,
            },
            _ => panic!("status can be nothing other than 'Success' or 'Failure'"),
        })
}
