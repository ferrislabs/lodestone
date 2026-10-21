use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkRejection {
    Status,
    RedirectedHome,
    Soft404Baseline,
    Soft404Marker,
    EmptyContent,
}

impl LinkRejection {
    pub const ALL: [Self; 5] = [
        Self::Status,
        Self::RedirectedHome,
        Self::Soft404Baseline,
        Self::Soft404Marker,
        Self::EmptyContent,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::RedirectedHome => "redirected_home",
            Self::Soft404Baseline => "soft_404_baseline",
            Self::Soft404Marker => "soft_404_marker",
            Self::EmptyContent => "empty_content",
        }
    }
}

impl fmt::Display for LinkRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnconfirmedLinks {
    pub status: u64,
    pub redirected_home: u64,
    pub soft_404_baseline: u64,
    pub soft_404_marker: u64,
    pub empty_content: u64,
}

impl UnconfirmedLinks {
    fn slot(&mut self, reason: LinkRejection) -> &mut u64 {
        match reason {
            LinkRejection::Status => &mut self.status,
            LinkRejection::RedirectedHome => &mut self.redirected_home,
            LinkRejection::Soft404Baseline => &mut self.soft_404_baseline,
            LinkRejection::Soft404Marker => &mut self.soft_404_marker,
            LinkRejection::EmptyContent => &mut self.empty_content,
        }
    }

    pub fn record(&mut self, reason: LinkRejection) {
        let slot = self.slot(reason);
        *slot = slot.saturating_add(1);
    }

    pub fn count(&self, reason: LinkRejection) -> u64 {
        match reason {
            LinkRejection::Status => self.status,
            LinkRejection::RedirectedHome => self.redirected_home,
            LinkRejection::Soft404Baseline => self.soft_404_baseline,
            LinkRejection::Soft404Marker => self.soft_404_marker,
            LinkRejection::EmptyContent => self.empty_content,
        }
    }

    pub fn merge(&mut self, other: &Self) {
        for reason in LinkRejection::ALL {
            let slot = self.slot(reason);
            *slot = slot.saturating_add(other.count(reason));
        }
    }

    pub fn total(&self) -> u64 {
        LinkRejection::ALL
            .iter()
            .fold(0u64, |sum, reason| sum.saturating_add(self.count(*reason)))
    }
}

impl fmt::Display for UnconfirmedLinks {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = LinkRejection::ALL
            .iter()
            .map(|reason| format!("{}={}", reason.as_str(), self.count(*reason)))
            .collect();
        formatter.write_str(&parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejections_are_counted_by_reason_and_merged() {
        let mut links = UnconfirmedLinks::default();
        links.record(LinkRejection::Status);
        links.record(LinkRejection::Soft404Marker);
        links.record(LinkRejection::Soft404Marker);
        assert_eq!(links.count(LinkRejection::Soft404Marker), 2);
        assert_eq!(links.total(), 3);
        let mut other = UnconfirmedLinks::default();
        other.record(LinkRejection::EmptyContent);
        links.merge(&other);
        assert_eq!(links.empty_content, 1);
        assert_eq!(links.total(), 4);
        assert_eq!(
            links.to_string(),
            "status=1 redirected_home=0 soft_404_baseline=0 soft_404_marker=2 empty_content=1"
        );
    }

    #[test]
    fn reasons_have_stable_names() {
        let names: Vec<&str> = LinkRejection::ALL
            .iter()
            .map(|reason| reason.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "status",
                "redirected_home",
                "soft_404_baseline",
                "soft_404_marker",
                "empty_content"
            ]
        );
    }
}
