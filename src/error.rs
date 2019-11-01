#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot get access token because: {0}")]
    GetAccessToken(#[source] attohttpc::Error),

    #[error("cannot deserialize response because: {0}")]
    Deserialize(#[source] attohttpc::Error),

    #[error("cannot get playlist because: {0}")]
    GetPlaylist(#[source] attohttpc::Error),

    #[error("cannot get response body because: {0}")]
    GetResponseBody(#[source] attohttpc::Error),

    #[error("cannot playlist")]
    InvalidPlaylist,

    #[error("cannot find token")]
    FindToken,

    #[error("cannot find signature")]
    FindSignature,
}
