// Copyright 2021 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: Apache-2.0

use crate::error::ErrorKind;
use crate::requests::ClientRequest;
use crate::responses::ServerResponse;
use nymsphinx::addressing::clients::Recipient;
use nymsphinx::anonymous_replies::ReplySurb;
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};

// local text equivalent of `ClientRequest` for easier serialization + deserialization with serde
// TODO: figure out if there's an easy way to avoid defining it

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum ClientRequestText {
    #[serde(rename_all = "camelCase")]
    Send {
        message: String,
        recipient: String,
        with_reply_surb: bool,
        connection_id: u64,
    },
    SelfAddress,
    #[serde(rename_all = "camelCase")]
    Reply {
        message: String,
        reply_surb: String,
    },
}

impl TryFrom<String> for ClientRequestText {
    type Error = serde_json::Error;

    fn try_from(msg: String) -> Result<Self, Self::Error> {
        serde_json::from_str(&msg)
    }
}

impl TryInto<ClientRequest> for ClientRequestText {
    type Error = crate::error::Error;

    fn try_into(self) -> Result<ClientRequest, Self::Error> {
        match self {
            ClientRequestText::Send {
                message,
                recipient,
                with_reply_surb,
                connection_id,
            } => {
                let message_bytes = message.into_bytes();
                let recipient = Recipient::try_from_base58_string(recipient).map_err(|err| {
                    Self::Error::new(ErrorKind::MalformedRequest, err.to_string())
                })?;

                Ok(ClientRequest::Send {
                    message: message_bytes,
                    recipient,
                    with_reply_surb,
                    connection_id,
                })
            }
            ClientRequestText::SelfAddress => Ok(ClientRequest::SelfAddress),
            ClientRequestText::Reply {
                message,
                reply_surb,
            } => {
                let message_bytes = message.into_bytes();
                let reply_surb = ReplySurb::from_base58_string(reply_surb).map_err(|err| {
                    Self::Error::new(ErrorKind::MalformedRequest, err.to_string())
                })?;

                Ok(ClientRequest::Reply {
                    message: message_bytes,
                    reply_surb,
                })
            }
        }
    }
}

// local text equivalent of `ServerResponse` for easier serialization + deserialization with serde
// TODO: figure out if there's an easy way to avoid defining it

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum ServerResponseText {
    #[serde(rename_all = "camelCase")]
    Received {
        message: String,
        reply_surb: Option<String>,
    },
    SelfAddress {
        address: String,
    },
    Error {
        message: String,
    },
}

impl TryFrom<String> for ServerResponseText {
    type Error = serde_json::Error;

    fn try_from(msg: String) -> Result<Self, <ServerResponseText as TryFrom<String>>::Error> {
        serde_json::from_str(&msg)
    }
}

impl From<ServerResponseText> for String {
    fn from(res: ServerResponseText) -> Self {
        // per serde_json docs:
        /*
        /// Serialization can fail if `T`'s implementation of `Serialize` decides to
        /// fail, or if `T` contains a map with non-string keys.
         */
        // this is not the case here.
        serde_json::to_string(&res).unwrap()
    }
}

impl From<ServerResponse> for ServerResponseText {
    fn from(resp: ServerResponse) -> Self {
        match resp {
            ServerResponse::Received(reconstructed) => {
                ServerResponseText::Received {
                    // TODO: ask DH what is more appropriate, lossy utf8 conversion or returning error and then
                    // pure binary later
                    message: String::from_utf8_lossy(&reconstructed.message).into_owned(),
                    reply_surb: reconstructed
                        .reply_surb
                        .map(|reply_surb| reply_surb.to_base58_string()),
                }
            }
            ServerResponse::SelfAddress(recipient) => ServerResponseText::SelfAddress {
                address: recipient.to_string(),
            },
            ServerResponse::Error(err) => ServerResponseText::Error {
                message: err.to_string(),
            },
        }
    }
}
