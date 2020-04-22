use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;

pub struct Taskwarrior {
    output: ButtonWidget,
    id: String,
    update_interval: Duration,
    warning_threshold: u32,
    critical_threshold: u32,
    filter_tags: Vec<String>,
    block_mode: TaskwarriorBlockMode,

    //useful, but optional
    #[allow(dead_code)]
    config: Config,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct TaskwarriorConfig {
    /// Update interval in seconds
    #[serde(
        default = "TaskwarriorConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    // Threshold from which on the block is marked with a warning indicator
    #[serde(default = "TaskwarriorConfig::default_threshold_warning")]
    pub warning_threshold: u32,

    // Threshold from which on the block is marked with a critical indicator
    #[serde(default = "TaskwarriorConfig::default_threshold_critical")]
    pub critical_threshold: u32,

    // A list of tags a task has to have before it's used for counting pending tasks
    #[serde(default = "TaskwarriorConfig::default_filter_tags")]
    pub filter_tags: Vec<String>,
}

enum TaskwarriorBlockMode {
    // Show only the tasks which are filtered by the set tags and which are not completed.
    OnlyFilteredPendingTasks,
    // Show all pending tasks and ignore the filtering tags.
    AllPendingTasks,
}

impl TaskwarriorConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(600)
    }

    fn default_threshold_warning() -> u32 {
        10
    }

    fn default_threshold_critical() -> u32 {
        20
    }

    fn default_filter_tags() -> Vec<String> {
        vec![]
    }
}

impl ConfigBlock for Taskwarrior {
    type Config = TaskwarriorConfig;

    fn new(
        block_config: Self::Config,
        config: Config,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(Taskwarrior {
            id: Uuid::new_v4().to_simple().to_string(),
            update_interval: block_config.interval,
            warning_threshold: block_config.warning_threshold,
            critical_threshold: block_config.critical_threshold,
            filter_tags: block_config.filter_tags,
            block_mode: TaskwarriorBlockMode::OnlyFilteredPendingTasks,
            output: ButtonWidget::new(config.clone(), "taskwarrior")
                .with_icon("tasks")
                .with_text("-"),
            tx_update_request,
            config,
        })
    }
}

fn has_taskwarrior() -> Result<bool> {
    Ok(String::from_utf8(
        Command::new("sh")
            .args(&["-c", "type -P task"])
            .output()
            .block_error(
                "taskwarrior",
                "failed to start command to check for taskwarrior",
            )?
            .stdout,
    )
    .block_error("taskwarrior", "failed to check for taskwarrior")?
    .trim()
        != "")
}

fn tags_to_filter(tags: &Vec<String>) -> String {
    tags.iter()
        .map(|element| format!("+{}", element))
        .collect::<Vec<String>>()
        .join(" ")
}

fn get_number_of_pending_tasks(tags: &Vec<String>) -> Result<u32> {
    String::from_utf8(
        Command::new("sh")
            .args(&[
                "-c",
                &format!("task -COMPLETED {} count", tags_to_filter(tags)),
            ])
            .output()
            .block_error(
                "taskwarrior",
                "failed to run taskwarrior for getting the number of pending tasks",
            )?
            .stdout,
    )
    .block_error(
        "taskwarrior",
        "failed to get the number of pending tasks from taskwarrior",
    )?
    .trim()
    .parse::<u32>()
    .block_error("taskwarrior", "could not parse the result of taskwarrior")
}

impl Block for Taskwarrior {
    fn update(&mut self) -> Result<Option<Duration>> {
        // if the taskwarrior binary is not installed, set the output to a questionmark
        if !has_taskwarrior()? {
            self.output.set_text("?")
        } else {
            let filter_tags = match self.block_mode {
                TaskwarriorBlockMode::OnlyFilteredPendingTasks => self.filter_tags.clone(),
                TaskwarriorBlockMode::AllPendingTasks => vec![],
            };
            let number_of_pending_tasks = get_number_of_pending_tasks(&filter_tags)?;
            self.output.set_text(format!("{}", number_of_pending_tasks));
            if number_of_pending_tasks >= self.critical_threshold {
                self.output.set_state(State::Critical);
            } else if number_of_pending_tasks >= self.warning_threshold {
                self.output.set_state(State::Warning);
            } else {
                self.output.set_state(State::Idle);
            }
        }

        // continue updating the block in the configured interval
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event
            .name
            .as_ref()
            .map(|s| s == "taskwarrior")
            .unwrap_or(false)
        {
            match event.button {
                MouseButton::Left => {
                    self.update()?;
                }
                MouseButton::Right => {
                    match self.block_mode {
                        TaskwarriorBlockMode::OnlyFilteredPendingTasks => {
                            self.block_mode = TaskwarriorBlockMode::AllPendingTasks
                        }
                        TaskwarriorBlockMode::AllPendingTasks => {
                            self.block_mode = TaskwarriorBlockMode::OnlyFilteredPendingTasks
                        }
                    }
                    self.update()?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}
