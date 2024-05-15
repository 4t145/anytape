use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    error::Error,
    future::Future,
    pin::Pin,
    sync::Arc,
};
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Protocol {
    expr: Cow<'static, [u8]>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Identity {
    expr: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Address {
    pub protocol: Protocol,
    pub identity: Identity,
}

pub struct Node {
    pub address_set: Vec<Address>,
}

pub struct Sender {}
pub struct Message {
    pub destination: Address,
    pub path: Vec<PathNode>,
    pub payload: Vec<u8>,
    pub signature: Vec<u8>,
    pub unique_id: u64,
}

pub struct PathNode {
    pub name: Option<String>,
    pub address: Option<Address>,
    pub ts: u64,
}

impl Default for PathNode {
    fn default() -> Self {
        Self::new()
    }
}

impl PathNode {
    pub fn new() -> Self {
        Self {
            name: None,
            address: None,
            ts: 0,
        }
    }
    pub fn with_name(self, name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..self
        }
    }
    pub fn with_address(self, address: impl Into<Address>) -> Self {
        Self {
            address: Some(address.into()),
            ..self
        }
    }
}
pub trait ProtocolExecutor {
    type Error: std::error::Error + Send + 'static + Sized;
    fn send(
        &self,
        remote: &Identity,
        message: Message,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static;
    fn get_status(
        &self,
        remote: &Identity,
        message: Message,
    ) -> impl Future<Output = Result<MessageStatus, Self::Error>> + Send + 'static;
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;
type BoxError = Box<dyn Error + Send + 'static>;
type BoxResult<T> = Result<T, BoxError>;
pub trait DynProtocolExecutor {
    fn send(&self, remote: &Identity, message: Message) -> BoxFuture<BoxResult<()>>;
}

impl<T> DynProtocolExecutor for T
where
    T: ProtocolExecutor,
{
    fn send(
        &self,
        remote: &Identity,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + 'static>>> + Send + 'static>>
    {
        let fut = ProtocolExecutor::send(self, remote, message);
        Box::pin(async move {
            let result = fut.await;
            match result {
                Ok(()) => Ok(()),
                Err(e) => Err(Box::new(e) as Box<dyn Error + Send + 'static>),
            }
        })
    }
}

pub struct NodeInstance {
    anon: bool,
    name: Option<String>,
    address_set: HashSet<Address>,
    next_cache: HashMap<Address, Address>,
    protocol_executor: HashMap<Protocol, Arc<dyn DynProtocolExecutor>>,
}

pub trait DataBackend {
    fn get_next(&self, addr: &Address) -> BoxFuture<BoxResult<Option<Address>>>;
    fn set_next(
        &self,
        addr: &Address,
        next: Option<&Address>,
    ) -> BoxFuture<BoxResult<Option<Address>>>;
}

pub enum MessageStatus {
    Sended,
    Received,
    Unreachable,
    SendError,
}

pub enum SendError {
    ExecutorError(BoxError),
    ProtocolNotSupport { supported: Vec<Protocol> },
}

impl NodeInstance {
    pub fn mark(&self, accept_at: Address, message: &mut Message) {
        let this_node = if self.anon {
            PathNode::new()
        } else {
            let mut pn = PathNode::new().with_address(accept_at);
            if let Some(name) = &self.name {
                pn = pn.with_name(name)
            }
            pn
        };
        message.path.push(this_node)
    }
    pub async fn send(&self, message: Message, to: Address) -> Result<(), SendError> {
        if let Some(executor) = self.protocol_executor.get(&to.protocol) {
            executor
                .send(&to.identity, message)
                .await
                .map_err(SendError::ExecutorError)
        } else {
            Err(SendError::ProtocolNotSupport {
                supported: self.protocol_executor.keys().cloned().collect(),
            })
        }
    }
}
