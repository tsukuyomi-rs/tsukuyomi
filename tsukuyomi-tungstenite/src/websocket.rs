use {
    futures::{Async, AsyncSink, Poll, Sink, StartSend, Stream},
    std::io,
    tsukuyomi::upgrade::Upgraded,
    tungstenite::{
        error::Error as WsError,
        protocol::{Message, WebSocketContext},
    },
};

pub struct WebSocket<'a> {
    stream: Wrapped<'a>,
    protocol: &'a mut WebSocketContext,
}

impl<'a> WebSocket<'a> {
    pub fn new(stream: &'a mut dyn Upgraded, protocol: &'a mut WebSocketContext) -> Self {
        Self {
            stream: Wrapped(stream),
            protocol,
        }
    }
}

impl<'a> Stream for WebSocket<'a> {
    type Item = Message;
    type Error = WsError;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        into_poll(self.protocol.read_message(&mut self.stream).map(Some))
    }
}

impl<'a> Sink for WebSocket<'a> {
    type SinkItem = Message;
    type SinkError = WsError;

    fn start_send(&mut self, msg: Message) -> StartSend<Self::SinkItem, Self::SinkError> {
        into_start_send(self.protocol.write_message(&mut self.stream, msg))
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        into_poll(self.protocol.write_pending(&mut self.stream))
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        into_poll(self.protocol.close(&mut self.stream, None))
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

struct Wrapped<'a>(&'a mut dyn Upgraded);

impl<'a> io::Read for Wrapped<'a> {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        self.0.read(dst)
    }
}

impl<'a> io::Write for Wrapped<'a> {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        self.0.write(src)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
