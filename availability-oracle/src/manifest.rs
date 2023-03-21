use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Link {
    #[serde(rename = "/")]
    pub(crate) link: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct File {
    pub(crate) file: Link,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Abi {
    pub(crate) file: Link,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Mapping {
    pub(crate) file: Link,
    pub(crate) abis: Vec<Abi>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct DataSource {
    pub(crate) kind: String,
    pub(crate) network: String,
    pub(crate) mapping: Mapping,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Manifest {
    pub(crate) schema: File,
    data_sources: Vec<DataSource>,
    templates: Option<Vec<DataSource>>,
}

impl Manifest {
    pub(crate) fn data_sources(&self) -> impl Iterator<Item = &DataSource> {
        self.data_sources
            .iter()
            .chain(self.templates.iter().flatten())
    }
}
