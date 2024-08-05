use url::Url;

pub struct Project {
    pub name: String,
    pub owner: String,
    pub changelog: Url,
}

impl Project {
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}