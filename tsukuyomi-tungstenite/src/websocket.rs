use {
    futures::{Async, AsyncSink, Poll, Sink, StartSend, Stream},
    std::io,
    tungstenite::{
        error::Error as WsError,
        protocol::{Message, WebSocketContext},
    },
};

pub struct WebSocket<'a, I> {
    stream: &'a mut I,
    protocol: &'a mut WebSocketContext,
}

impl<'a, I> WebSocket<'a, I>
where
    I: io::Read + io::Write,
{
    pub fn new(stream: &'a mut I, protocol: &'a mut WebSocketContext) -> Self {
        Self { stream, protocol }
    }
}

impl<'a, I> Stream for WebSocket<'a, I>
where
    I: io::Read + io::Write,
{
    type Item = Message;
    type Error = WsError;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        into_poll(self.protocol.read_message(self.stream).map(Some))
    }
}

impl<'a, I> Sink for WebSocket<'a, I>
where
    I: io::Read + io::Write,
{
    type SinkItem = Message;
    type SinkError = WsError;

    fn start_send(&mut self, msg: Message) -> StartSend<Self::SinkItem, Self::SinkError> {
        into_start_send(self.protocol.write_message(self.stream, msg))
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        into_poll(self.protocol.write_pending(self.stream))
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        into_poll(self.protocol.close(self.stream, None))
    }
}

fn into_poll<T>(res: Result<T, WsError>) -> Poll<T, WsError> {
    match res {
        Ok(val) => Ok(Async::Ready(val)),
        Err(WsError::Io(ref e)) if e.kind() == io::ErrorKind::WouldBlock => Ok(Async::NotReady),
        Err(e) => Err(e),
    }
}

fn into_start_send(res: Result<(), WsError>) -> StartSend<Message, WsError> {
    match res {
        Ok(()) => Ok(AsyncSink::Ready),
        Err(WsError::Io(ref e)) if e.kind() == io::ErrorKind::WouldBlock => Ok(AsyncSink::Ready),
        Err(WsError::SendQueueFull(msg)) => Ok(AsyncSink::NotReady(msg)),
        Err(e) => Err(e),
    }
}
