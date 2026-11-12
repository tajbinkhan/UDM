use udm_domain::ports::{Clock, FilesError, GrabProjects, GrabRuns};

/// A grabber project its schedule started.
#[derive(Debug, PartialEq, Eq)]
pub struct GrabScheduled {
    pub project: String,
    /// The project file could not be saved, so a restart may start the
    /// same run again.
    pub not_saved: Option<FilesError>,
}

/// Start the saved grabber projects whose scheduled run is due, each in
/// the background, noting the run in the project first.
pub struct RunGrabSchedules<'a, P, R, C> {
    projects: &'a P,
    runs: &'a R,
    clock: &'a C,
}

impl<'a, P: GrabProjects, R: GrabRuns, C: Clock> RunGrabSchedules<'a, P, R, C> {
    pub fn new(projects: &'a P, runs: &'a R, clock: &'a C) -> Self {
        RunGrabSchedules { projects, runs, clock }
    }

    pub fn run(&self) -> Vec<GrabScheduled> {
        let now = self.clock.now();
        let mut started = Vec::new();
        for name in self.projects.names() {
            let Some(mut project) = self.projects.load(&name) else {
                continue;
            };
            let Some(day) = project.due(now) else {
                continue;
            };
            if !self.runs.claim(&name) {
                continue;
            }
            // Saved first, so a restart does not start the same run again.
            project.start_scheduled_run(day, now);
            let not_saved = self.projects.save(&project).err();
            self.runs.run(project, day);
            started.push(GrabScheduled { project: name, not_saved });
        }
        started
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::FIXED_NOW;
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;
    use udm_domain::grab::{GrabRequest, Project};
    use udm_domain::queue::Schedule;

    #[derive(Default)]
    struct Projects(Mutex<HashMap<String, Project>>);

    impl GrabProjects for Projects {
        fn names(&self) -> Vec<String> {
            let mut names: Vec<String> = self.0.lock().unwrap().keys().cloned().collect();
            names.sort();
            names
        }

        fn load(&self, name: &str) -> Option<Project> {
            self.0.lock().unwrap().get(name).cloned()
        }

        fn save(&self, project: &Project) -> Result<(), FilesError> {
            self.0.lock().unwrap().insert(project.name.clone(), project.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct Runs {
        going: Mutex<HashSet<String>>,
        ran: Mutex<Vec<(String, i64)>>,
    }

    impl GrabRuns for Runs {
        fn claim(&self, name: &str) -> bool {
            self.going.lock().unwrap().insert(name.to_string())
        }

        fn run(&self, project: Project, day: i64) {
            self.ran.lock().unwrap().push((project.name, day));
        }
    }

    fn project(name: &str, start: Option<&str>) -> Project {
        let schedule = start.map(|s| Schedule {
            start: s.into(),
            stop: None,
            days: (0..7).collect(),
            date: None,
            repeat_minutes: None,
            wake: false,
        });
        let request = GrabRequest { start: "https://s/".into(), ..GrabRequest::default() };
        let json = serde_json::json!({ "name": name, "request": request, "schedule": schedule });
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn due_projects_start_once_and_note_their_run() {
        let projects = Projects::default();
        for p in [project("Due", Some("09:00")), project("Later", Some("18:00")), project("Manual", None)] {
            projects.save(&p).unwrap();
        }
        let runs = Runs::default();
        // FIXED_NOW is 09:30.
        let started = RunGrabSchedules::new(&projects, &runs, &FIXED_NOW).run();
        assert_eq!(started, [GrabScheduled { project: "Due".into(), not_saved: None }]);
        assert_eq!(*runs.ran.lock().unwrap(), [("Due".to_string(), FIXED_NOW.0.day)]);
        assert_eq!(projects.load("Due").unwrap().last_scheduled_day, Some(FIXED_NOW.0.day));
        assert!(RunGrabSchedules::new(&projects, &runs, &FIXED_NOW).run().is_empty(), "today's run happened");
    }

    #[test]
    fn a_project_already_running_is_not_started_again() {
        let projects = Projects::default();
        projects.save(&project("Due", Some("09:00"))).unwrap();
        let runs = Runs::default();
        runs.claim("Due");
        assert!(RunGrabSchedules::new(&projects, &runs, &FIXED_NOW).run().is_empty());
        assert_eq!(projects.load("Due").unwrap().last_scheduled_day, None);
    }
}
