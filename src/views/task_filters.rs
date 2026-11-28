use crate::date;
use crate::odoo::Task;
use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum DoneFilter {
    #[default]
    All,
    Open,
    Done,
}

impl DoneFilter {
    pub const ALL: [(Self, &'static str); 3] = [
        (Self::All, "All"),
        (Self::Open, "Open"),
        (Self::Done, "Done"),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Open => "Open only",
            Self::Done => "Done only",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum DueFilter {
    #[default]
    Any,
    Overdue,
    Today,
    ThisWeek,
    NoDeadline,
}

impl DueFilter {
    pub const ALL: [(Self, &'static str); 5] = [
        (Self::Any, "Any"),
        (Self::Overdue, "Overdue"),
        (Self::Today, "Today"),
        (Self::ThisWeek, "This week"),
        (Self::NoDeadline, "No due date"),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Any => "Any",
            Self::Overdue => "Overdue",
            Self::Today => "Due today",
            Self::ThisWeek => "Due this week",
            Self::NoDeadline => "No due date",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum HoursFilter {
    #[default]
    Any,
    Logged,
    Unlogged,
}

impl HoursFilter {
    pub const ALL: [(Self, &'static str); 3] = [
        (Self::Any, "Any"),
        (Self::Logged, "Has time"),
        (Self::Unlogged, "No time"),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Any => "Any",
            Self::Logged => "Has logged time",
            Self::Unlogged => "No time logged",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum KindFilter {
    #[default]
    All,
    TopLevel,
    SubTasks,
}

impl KindFilter {
    pub const ALL: [(Self, &'static str); 3] = [
        (Self::All, "All"),
        (Self::TopLevel, "Top-level"),
        (Self::SubTasks, "Sub-tasks"),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::TopLevel => "Top-level only",
            Self::SubTasks => "Sub-tasks only",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum SortBy {
    #[default]
    Default,
    Deadline,
    LoggedHours,
    StageAge,
    Name,
}

impl SortBy {
    pub const ALL: [(Self, &'static str); 5] = [
        (Self::Default, "Default"),
        (Self::Deadline, "Due date"),
        (Self::LoggedHours, "Most hours"),
        (Self::StageAge, "Longest in stage"),
        (Self::Name, "Name A–Z"),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Deadline => "Due date",
            Self::LoggedHours => "Most hours",
            Self::StageAge => "Longest in stage",
            Self::Name => "Name A–Z",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupBy {
    #[default]
    None,
    Status,
    Assignee,
    Project,
    Customer,
    Due,
    AssignedBy,
    Priority,
}

impl GroupBy {
    pub const ALL: [(Self, &'static str); 8] = [
        (Self::None, "None"),
        (Self::Status, "Status"),
        (Self::Assignee, "Assignee"),
        (Self::Project, "Project"),
        (Self::Customer, "Customer"),
        (Self::Due, "Due date"),
        (Self::AssignedBy, "Assigned by"),
        (Self::Priority, "Priority"),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Status => "Status",
            Self::Assignee => "Assignee",
            Self::Project => "Project",
            Self::Customer => "Customer",
            Self::Due => "Due date",
            Self::AssignedBy => "Assigned by",
            Self::Priority => "Priority",
        }
    }
}

#[derive(Default)]
pub struct TaskFilters {
    pub done: DoneFilter,
    pub due: DueFilter,
    pub hours: HoursFilter,
    pub kind: KindFilter,
    pub high_only: bool,
    pub unassigned_only: bool,
    pub tags: HashSet<String>,
    pub customer: Option<String>,
    pub sort: SortBy,
    pub group: GroupBy,
    pub show_stats: bool,
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Aggregate {
    pub total: usize,
    pub open: usize,
    pub done: usize,
    pub overdue: usize,
    pub due_today: usize,
    pub high: usize,
    pub unassigned: usize,
    pub hours: f64,
}

impl Aggregate {
    pub fn compute(tasks: &[&Task], today: i64) -> Self {
        let mut out = Self {
            total: tasks.len(),
            ..Self::default()
        };
        for t in tasks {
            if t.done {
                out.done += 1;
            } else {
                out.open += 1;
                if let Some(days) = t.deadline.as_deref().and_then(date::days_from_iso) {
                    if days < today {
                        out.overdue += 1;
                    } else if days == today {
                        out.due_today += 1;
                    }
                }
            }
            if t.priority_high {
                out.high += 1;
            }
            if t.assignees.is_empty() {
                out.unassigned += 1;
            }
            out.hours += t.logged_hours;
        }
        out
    }

    pub fn completion_pct(&self) -> usize {
        (self.done * 100 + self.total / 2)
            .checked_div(self.total)
            .unwrap_or(0)
    }
}

impl TaskFilters {
    pub fn clear(&mut self) {
        self.done = DoneFilter::All;
        self.due = DueFilter::Any;
        self.hours = HoursFilter::Any;
        self.kind = KindFilter::All;
        self.high_only = false;
        self.unassigned_only = false;
        self.tags.clear();
        self.customer = None;
        self.sort = SortBy::Default;
        self.group = GroupBy::None;
        self.show_stats = false;
    }

    pub fn active_count(&self) -> usize {
        usize::from(self.done != DoneFilter::All)
            + usize::from(self.due != DueFilter::Any)
            + usize::from(self.hours != HoursFilter::Any)
            + usize::from(self.kind != KindFilter::All)
            + usize::from(self.high_only)
            + usize::from(self.unassigned_only)
            + self.tags.len()
            + usize::from(self.customer.is_some())
            + usize::from(self.sort != SortBy::Default)
            + usize::from(self.group != GroupBy::None)
            + usize::from(self.show_stats)
    }

    pub fn matches(&self, t: &Task, today: i64, known_ids: &HashSet<i64>) -> bool {
        let done_ok = match self.done {
            DoneFilter::All => true,
            DoneFilter::Open => !t.done,
            DoneFilter::Done => t.done,
        };
        let due_days = t.deadline.as_deref().and_then(date::days_from_iso);
        let due_ok = match self.due {
            DueFilter::Any => true,
            DueFilter::Overdue => !t.done && due_days.is_some_and(|d| d < today),
            DueFilter::Today => due_days == Some(today),
            DueFilter::ThisWeek => due_days.is_some_and(|d| d >= today && d <= today + 6),
            DueFilter::NoDeadline => due_days.is_none(),
        };
        let hours_ok = match self.hours {
            HoursFilter::Any => true,
            HoursFilter::Logged => t.logged_hours > 0.,
            HoursFilter::Unlogged => t.logged_hours <= 0.,
        };
        let is_sub = t
            .parent
            .as_ref()
            .is_some_and(|(pid, _)| known_ids.contains(pid));
        let kind_ok = match self.kind {
            KindFilter::All => true,
            KindFilter::TopLevel => !is_sub,
            KindFilter::SubTasks => is_sub,
        };
        let tags_ok = self.tags.is_empty() || t.tags.iter().any(|(_, n)| self.tags.contains(n));
        let customer_ok = self
            .customer
            .as_deref()
            .is_none_or(|c| t.partner.as_ref().is_some_and(|(_, name)| name == c));

        done_ok
            && due_ok
            && hours_ok
            && kind_ok
            && (!self.high_only || t.priority_high)
            && (!self.unassigned_only || t.assignees.is_empty())
            && tags_ok
            && customer_ok
    }

    pub fn sort_indices(&self, indices: &mut [usize], tasks: &[Task], today: i64) {
        match self.sort {
            SortBy::Default => {}
            SortBy::Deadline => indices.sort_by(|&a, &b| {
                let da = tasks[a].deadline.as_deref().and_then(date::days_from_iso);
                let db = tasks[b].deadline.as_deref().and_then(date::days_from_iso);
                match (da, db) {
                    (Some(x), Some(y)) => x.cmp(&y),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => tasks[a]
                        .name
                        .to_lowercase()
                        .cmp(&tasks[b].name.to_lowercase()),
                }
            }),
            SortBy::LoggedHours => indices.sort_by(|&a, &b| {
                tasks[b]
                    .logged_hours
                    .total_cmp(&tasks[a].logged_hours)
                    .then_with(|| {
                        tasks[a]
                            .name
                            .to_lowercase()
                            .cmp(&tasks[b].name.to_lowercase())
                    })
            }),
            SortBy::StageAge => indices.sort_by(|&a, &b| {
                let aa = tasks[a]
                    .stage_since
                    .as_deref()
                    .and_then(|s| date::days_since(s, today));
                let ab = tasks[b]
                    .stage_since
                    .as_deref()
                    .and_then(|s| date::days_since(s, today));
                ab.cmp(&aa).then_with(|| {
                    tasks[a]
                        .name
                        .to_lowercase()
                        .cmp(&tasks[b].name.to_lowercase())
                })
            }),
            SortBy::Name => indices.sort_by(|&a, &b| {
                tasks[a]
                    .name
                    .to_lowercase()
                    .cmp(&tasks[b].name.to_lowercase())
            }),
        }
    }

    pub fn group_keys(&self, t: &Task, today: i64) -> Vec<String> {
        match self.group {
            GroupBy::None => Vec::new(),
            GroupBy::Status => vec![t.stage.clone()],
            GroupBy::Assignee => {
                if t.assignees.is_empty() {
                    vec!["Unassigned".into()]
                } else {
                    t.assignees.clone()
                }
            }
            GroupBy::Project => vec![t.project.clone()],
            GroupBy::Customer => vec![
                t.partner
                    .as_ref()
                    .map(|(_, name)| name.clone())
                    .unwrap_or_else(|| "No customer".into()),
            ],
            GroupBy::Due => vec![due_bucket(t, today).into()],
            GroupBy::AssignedBy => vec![t.creator.clone().unwrap_or_else(|| "Unknown".into())],
            GroupBy::Priority => vec![if t.priority_high {
                "High priority".into()
            } else {
                "Normal".into()
            }],
        }
    }

    pub fn group_order(
        &self,
        label: &str,
        count: usize,
    ) -> (usize, std::cmp::Reverse<usize>, String) {
        let rank = match self.group {
            GroupBy::Due => match label {
                "Overdue" => 0,
                "Today" => 1,
                "This week" => 2,
                "Later" => 3,
                "No due date" => 4,
                "Completed" => 5,
                _ => 6,
            },
            GroupBy::Priority => match label {
                "High priority" => 0,
                _ => 1,
            },
            GroupBy::Assignee | GroupBy::AssignedBy => match label {
                "Me" => 0,
                "Unassigned" | "Unknown" => 2,
                _ => 1,
            },
            GroupBy::Customer => usize::from(label == "No customer"),
            _ => 0,
        };
        (rank, std::cmp::Reverse(count), label.to_lowercase())
    }
}

fn due_bucket(t: &Task, today: i64) -> &'static str {
    if t.done {
        return "Completed";
    }
    match t.deadline.as_deref().and_then(date::days_from_iso) {
        Some(d) if d < today => "Overdue",
        Some(d) if d == today => "Today",
        Some(d) if d <= today + 6 => "This week",
        Some(_) => "Later",
        None => "No due date",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: i64) -> Task {
        Task {
            id,
            name: format!("Task {id}"),
            project: "Core".into(),
            project_id: Some(1),
            parent: None,
            sequence: 0,
            stage_id: None,
            stage: "In Progress".into(),
            state: "".into(),
            assignees: vec!["Me".into()],
            deadline: None,
            priority_high: false,
            logged_hours: 0.,
            done: false,
            stage_since: None,
            user_ids: vec![1],
            tags: vec![],
            partner: None,
            creator: Some("Me".into()),
            creator_id: Some(1),
            personal_stage: None,
        }
    }

    #[test]
    fn filters_and_aggregates_match_real_scenarios() {
        let today = date::days_from_iso("2026-11-28").unwrap();
        let mut t1 = sample(1);
        t1.deadline = Some("2026-11-25".into());
        t1.priority_high = true;
        t1.logged_hours = 3.5;

        let mut t2 = sample(2);
        t2.deadline = Some("2026-11-28".into());
        t2.assignees.clear();
        t2.tags = vec![(1, "Backend".into())];
        t2.partner = Some((9, "Acme".into()));

        let mut t3 = sample(3);
        t3.done = true;
        t3.deadline = Some("2026-11-20".into());
        t3.logged_hours = 1.5;

        let known: HashSet<i64> = [1, 2, 3].into();
        let all = [&t1, &t2, &t3];
        let agg = Aggregate::compute(&all, today);
        assert_eq!(agg.total, 3);
        assert_eq!(agg.open, 2);
        assert_eq!(agg.done, 1);
        assert_eq!(agg.overdue, 1); // t3 is done so not overdue
        assert_eq!(agg.due_today, 1);
        assert_eq!(agg.high, 1);
        assert_eq!(agg.unassigned, 1);
        assert_eq!(agg.hours, 5.0);
        assert_eq!(agg.completion_pct(), 33);

        let mut f = TaskFilters {
            due: DueFilter::Overdue,
            ..Default::default()
        };
        assert!(f.matches(&t1, today, &known));
        assert!(!f.matches(&t2, today, &known));
        assert!(!f.matches(&t3, today, &known));

        f.due = DueFilter::Any;
        f.unassigned_only = true;
        f.tags.insert("Backend".into());
        f.customer = Some("Acme".into());
        assert!(!f.matches(&t1, today, &known));
        assert!(f.matches(&t2, today, &known));
        assert_eq!(f.active_count(), 3);
    }

    #[test]
    fn sorting_and_due_buckets() {
        let today = date::days_from_iso("2026-11-28").unwrap();
        let mut a = sample(1);
        a.logged_hours = 1.;
        a.deadline = Some("2026-12-02".into());
        let mut b = sample(2);
        b.logged_hours = 5.;
        b.deadline = Some("2026-11-26".into());
        let tasks = [a, b];
        let mut idx = [0, 1];

        let f = TaskFilters {
            sort: SortBy::LoggedHours,
            group: GroupBy::Due,
            ..Default::default()
        };
        f.sort_indices(&mut idx, &tasks, today);
        assert_eq!(idx, [1, 0]);
        assert_eq!(f.group_keys(&tasks[0], today), vec!["This week"]);
        assert_eq!(f.group_keys(&tasks[1], today), vec!["Overdue"]);
    }
}
