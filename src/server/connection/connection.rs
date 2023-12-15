// Path: src/server/connection.rs
use crate::server::connection::message::message::{
    AcknowledgementMessage, AudioData, BasicCommand, CommandObject, ConnectMessage, CreateStream,
    Event, FCPublish, OnStatus, PauseMessage, PlayMessage, Publish, ReleaseStream, ResultObject,
    RtmpMessage, SetChunkSizeMessage, SetDataFrame, VideoData,
};
use crate::protocol;
use crate::protocol::{ChunkBasicHeader, ChunkMessageHeader};

use log::{error, info, warn};
use rand::{Rng, SeedableRng};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::error;

pub struct Connection {
    stream: TcpStream,
    chunk_header: ChunkMessageHeader,
    chunk_size: usize,
}

#[warn(unreachable_code)]
impl Connection {
    pub fn new(stream: TcpStream) -> Connection {
        Connection { stream, chunk_header: ChunkMessageHeader::default(), chunk_size: protocol::DEFAULT_CHUNK_SIZE }
    }

    pub async fn handle(&mut self) -> Result<(), Box<dyn error::Error>> {
        // Perform the RTMP handshake.
        self.handshake().await?;

        // Handle RTMP messages.
        // ...
        loop {
            info!("Listening for msg");
            let message = self.read_message().await?;

            match message {
                RtmpMessage::Connect(connect_message) => {
                    self.handle_connect(connect_message).await?;
                }
                RtmpMessage::CreateStream(create_stream_message) => {
                    self.handle_create_stream(create_stream_message).await?;
                }
                RtmpMessage::SetChunkSize(set_chunk_size_message) => {
                    self.chunk_size = set_chunk_size_message.chunk_size;
                }
                RtmpMessage::_Play(play_message) => {
                    self.handle_play(play_message).await?;
                }
                RtmpMessage::_Pause(pause_message) => {
                    self.handle_pause(pause_message).await?;
                }
                RtmpMessage::Publish(publish_message) => {
                    self.handle_publish(publish_message).await?;
                }
                RtmpMessage::AudioData(audio_data) => {
                    Self::handle_audio_data(audio_data.data)?;
                }
                RtmpMessage::VideoData(video_data) => {
                    Self::handle_video_data(video_data.data)?;
                }
                _ => {
                    error!("Unhandled message: {:?}", message);
                }
            }
        }
    }

    async fn handshake(&mut self) -> Result<(), Box<dyn error::Error>> {
        let mut c0_buffer = [0; 1];
        let mut c1_buffer = [0; protocol::HANDSHAKE_CHUNK_SIZE];
        let mut c2_buffer = [0; protocol::HANDSHAKE_CHUNK_SIZE];

        // Read C0 and C1 from the client.
        self.stream.read_exact(&mut c0_buffer).await?;
        self.stream.read_exact(&mut c1_buffer).await?;

        let (s0, s1, s2) = protocol::s_chunk(c0_buffer, c1_buffer)?;

        // Write S0, S1, and S2 to the client.
        self.stream.write_all(&s0).await?;
        self.stream.write_all(&s1).await?;
        self.stream.write_all(&s2).await?;

        // Read C2 from the client.
        self.stream.read_exact(&mut c2_buffer).await?;

        // Check that C2 matches S1.
        if c2_buffer != s1 {
            error!("C2 does not match S1");
            return Err("C2 does not match S1".into());
        }

        Ok(())
    }

    async fn handle_connect(
        &mut self,
        msg: ConnectMessage,
    ) -> Result<(), Box<dyn error::Error>> {
        // Handle a Connect message.
        // ...
        // send win ack size
        // connect to app
        // send set peer bandwidth
        // read ack
        // make and send StreamBegin
        // make and send _result
        info!("==========Start Connect msg Handle==========");
        info!("Connect message: {:?}", msg);

        self.stream.write_all(&protocol::ack_msg()?).await?;
        self.stream.write_all(&protocol::band_msg()?).await?;

        self.read_message().await?;

        info!("==========End Connect msg Handle==========");
        Ok(())
    }

    async fn handle_create_stream(
        &mut self,
        msg: CreateStream,
    ) -> Result<(), Box<dyn error::Error>> {
        // Handle a CreateStream message.
        // ...
        self.stream.write_all(&protocol::result_msg(msg)?).await?;
        Ok(())
    }

    async fn handle_play(&mut self, msg: PlayMessage) -> Result<(), Box<dyn error::Error>> {
        // Handle a Play message.
        // ...
        info!("Play message: {:?}", msg);
        Ok(())
    }

    async fn handle_pause(&mut self, msg: PauseMessage) -> Result<(), Box<dyn error::Error>> {
        // Handle a Pause message.
        // ...
        info!("Pause message: {:?}", msg);
        Ok(())
    }

    async fn handle_publish(&mut self, msg: Publish) -> Result<(), Box<dyn error::Error>> {
        // Handle a Publish message.
        // ...
        self.stream.write_all(&protocol::stream_begin_msg()?).await?;
        self.stream.write_all(&protocol::on_status_msg(msg)?).await?;
        Ok(())
    }

    fn handle_audio_data(data: Vec<u8>) -> Result<(), Box<dyn error::Error>> {
        // Handle audio data.
        // ...
        if data.len() < 65 {
            info!("Audio data: {:?}", &data[1..data.len()]);
        } else {
            info!("Audio data: {:?}", &data[1..64]);
        }
        Ok(())
    }

    fn handle_video_data(data: Vec<u8>) -> Result<(), Box<dyn error::Error>> {
        // Handle video data.
        // ...
        if data.len() < 65 {
            info!("Video data: {:?}", &data[1..data.len()]);
        } else {
            info!("Video data: {:?}", &data[1..64]);
        }
        Ok(())
    }

    async fn read_message(&mut self) -> Result<RtmpMessage, Box<dyn error::Error>> {
        self.parse_msg_header().await?; // Set the self.chunk_header with current message header info

        // Since message length can be larger than a chunk, we don't want to read past the current chunk
        let msg_size = std::cmp::min(self.chunk_header.message_length.unwrap() as usize, self.chunk_size);
        let mut msg = vec![0; msg_size];
        self.stream.read_exact(&mut msg).await?;

        let message = protocol::parse_message(self.chunk_header.clone(), msg)?;

        // Remember how much data we have left in a multi-chunk data block
        self.chunk_header.message_length = Some(self.chunk_header.message_length.unwrap() - msg_size as u32);

        return Ok(message);
    }

    pub async fn parse_msg_header(&mut self) -> Result<(), Box<dyn error::Error>> {
        let mut buffer = [0; 1]; // Read the Basic Header

        self.stream.read_exact(&mut buffer).await?;
        if buffer.len() < 1 {
            error!("No message to read");
            return Err("No message to read".into());
        }

        let basic_header = ChunkBasicHeader::new(&buffer[0]);
        info!("fmt: {}, cs: {}", basic_header.fmt, basic_header.cs);

        let header_size = protocol::header_size_from_type(&basic_header)?;
        let mut header = vec![0; header_size];
        self.stream.read_exact(&mut header).await?;

        // Read the rest of the header into self.chunk_header for later access
        self.chunk_header = protocol::read_header(&header, basic_header, self.chunk_header.clone())?;
        Ok(())
    }
}

#[allow(unused_mut)]
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    async fn setup() -> (Connection, TcpStream) {
        // Start a TcpListener to accept connections (server-side)
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Connect a TcpStream to the TcpListener (client-side)
        let mut client = TcpStream::connect(addr).await.unwrap();

        // Accept the connection on the server-side
        let (server, _) = listener.accept().await.unwrap();

        // Create the Connection instance
        let mut conn = Connection::new(server);

        (conn, client)
    }

    #[tokio::test]
    async fn test_read_connect() {
        let (mut conn, mut client) = setup().await;

        // Emulate the client sending data
        let mock_data: &[u8] = &[
            2, 0, 0, 0, 0, 0, 4, 1, 0, 0, 0, 0, 0, 0, 16, 0, 3, 0, 0, 0, 0, 0, 179, 20, 0, 0, 0, 0,
            2, 0, 7, 99, 111, 110, 110, 101, 99, 116, 0, 63, 240, 0, 0, 0, 0, 0, 0, 3, 0, 3, 97,
            112, 112, 2, 0, 4, 108, 105, 118, 101, 0, 4, 116, 121, 112, 101, 2, 0, 10, 110, 111,
            110, 112, 114, 105, 118, 97, 116, 101, 0, 8, 102, 108, 97, 115, 104, 86, 101, 114, 2,
            0, 31, 70, 77, 76, 69, 47, 51, 46, 48, 32, 40, 99, 111, 109, 112, 97, 116, 105, 98,
            108, 101, 59, 32, 70, 77, 83, 99, 47, 49, 46, 48, 41, 0, 6, 115, 119, 102, 85, 114,
            108, 2, 0, 30, 114, 116, 109, 112, 58, 47, 47, 49, 57, 50, 46, 49, 54, 56, 46, 49, 46,
            49, 49, 50, 58, 49, 57, 51, 53, 47, 108, 105, 118, 101, 0, 5, 116, 99, 85, 114, 108, 2,
            0, 30, 114, 116, 109, 112, 58, 47, 47, 49, 57, 50, 46, 49, 54, 56, 46, 49, 46, 49, 49,
            50, 58, 49, 57, 51, 53, 47, 108, 105, 118, 101, 0, 0, 9,
        ];
        client
            .write_all(mock_data)
            .await
            .expect("Failed to write mock data");

        // Read & handle the message in the Connection instance
        let message = conn.read_message().await.expect("Failed to read message");
        let result = match message {
            RtmpMessage::Connect(connect_message) => {
                assert_eq!(
                    connect_message.connect_object.app, "live",
                    "App should be 'live'"
                );
                assert_eq!(
                    connect_message.connect_object.flash_ver, "FMLE/3.0 (compatible; FMSc/1.0)",
                    "Flash version should be 'FMLE/3.0 (compatible; FMSc/1.0)'"
                );
                assert_eq!(
                    connect_message.connect_object.swf_url, "rtmp://192.168.1.112:1935/live",
                    "SWF URL should be 'rtmp://192.168.1.112:1935/live'"
                );
                assert_eq!(
                    connect_message.connect_object.tc_url, "rtmp://192.168.1.112:1935/live",
                    "TC URL should be 'rtmp://192.168.1.112:1935/live'"
                );
                assert_eq!(
                    connect_message.connect_object.stream_type, "nonprivate",
                    "Stream type should be 'nonprivate'"
                );
                assert_eq!(connect_message.id, 1, "ID should be 1");
                Ok(())
            }
            // ... handle other cases or use a default case.
            _ => Err(Box::<dyn error::Error>::from("Unknown message type")),
        };

        // Check the result
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_ack() {
        let (mut conn, mut client) = setup().await;

        // Emulate the client sending data
        let mock_data: &[u8] = &[66, 0, 0, 0, 0, 0, 4, 3, 0, 0, 12, 35];
        client
            .write_all(mock_data)
            .await
            .expect("Failed to write mock data");

        // Read & handle the message in the Connection instance
        let message = conn.read_message().await.expect("Failed to read message");
        let result = match message {
            RtmpMessage::Acknowledgement(ack) => {
                assert_eq!(ack.sequence_number, 3107, "Sequence number should be 3107");
                Ok(())
            }
            // ... handle other cases or use a default case.
            _ => Err(Box::<dyn error::Error>::from("Unknown message type")),
        };

        // Check the result
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_create() {
        let (mut conn, mut client) = setup().await;

        // Emulate the client sending data
        let mock_data: &[u8] = &[
            67, 0, 0, 0, 0, 0, 38, 20, 2, 0, 13, 114, 101, 108, 101, 97, 115, 101, 83, 116, 114,
            101, 97, 109, 0, 64, 0, 0, 0, 0, 0, 0, 0, 5, 2, 0, 9, 115, 116, 114, 101, 97, 109, 107,
            101, 121, 67, 0, 0, 0, 0, 0, 34, 20, 2, 0, 9, 70, 67, 80, 117, 98, 108, 105, 115, 104,
            0, 64, 8, 0, 0, 0, 0, 0, 0, 5, 2, 0, 9, 115, 116, 114, 101, 97, 109, 107, 101, 121, 67,
            0, 0, 0, 0, 0, 25, 20, 2, 0, 12, 99, 114, 101, 97, 116, 101, 83, 116, 114, 101, 97,
            109, 0, 64, 16, 0, 0, 0, 0, 0, 0, 5,
        ];
        client
            .write_all(mock_data)
            .await
            .expect("Failed to write mock data");

        // Read & handle the message in the Connection instance
        let message = conn.read_message().await.expect("Failed to read message");
        let result = match message {
            RtmpMessage::CreateStream(create_stream) => {
                assert_eq!(
                    create_stream.command_name, "createStream",
                    "Command name should be 'createStream'"
                );
                assert_eq!(
                    create_stream.transaction_id, 4,
                    "Transaction ID should be 4"
                );
                Ok(())
            }
            // ... handle other cases or use a default case.
            _ => Err(Box::<dyn error::Error>::from("Unknown message type")),
        };

        // Check the result
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_connect() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind");
        let server_addr = listener.local_addr().unwrap();

        // Simulate the client
        tokio::spawn(async move {
            let mut client_stream = TcpStream::connect(server_addr)
                .await
                .expect("Failed to connect");
            let mock_data: &[u8] = &[
                2, 0, 0, 0, 0, 0, 4, 1, 0, 0, 0, 0, 0, 0, 16, 0, 3, 0, 0, 0, 0, 0, 179, 20, 0, 0,
                0, 0, 2, 0, 7, 99, 111, 110, 110, 101, 99, 116, 0, 63, 240, 0, 0, 0, 0, 0, 0, 3, 0,
                3, 97, 112, 112, 2, 0, 4, 108, 105, 118, 101, 0, 4, 116, 121, 112, 101, 2, 0, 10,
                110, 111, 110, 112, 114, 105, 118, 97, 116, 101, 0, 8, 102, 108, 97, 115, 104, 86,
                101, 114, 2, 0, 31, 70, 77, 76, 69, 47, 51, 46, 48, 32, 40, 99, 111, 109, 112, 97,
                116, 105, 98, 108, 101, 59, 32, 70, 77, 83, 99, 47, 49, 46, 48, 41, 0, 6, 115, 119,
                102, 85, 114, 108, 2, 0, 30, 114, 116, 109, 112, 58, 47, 47, 49, 57, 50, 46, 49,
                54, 56, 46, 49, 46, 49, 49, 50, 58, 49, 57, 51, 53, 47, 108, 105, 118, 101, 0, 5,
                116, 99, 85, 114, 108, 2, 0, 30, 114, 116, 109, 112, 58, 47, 47, 49, 57, 50, 46,
                49, 54, 56, 46, 49, 46, 49, 49, 50, 58, 49, 57, 51, 53, 47, 108, 105, 118, 101, 0,
                0, 9,
            ]; // Your mock RTMP connect data
            client_stream
                .write_all(mock_data)
                .await
                .expect("Failed to send mock data");

            // Read server responses, send ack, etc.
            // ...
            // Read server responses
            let mut response_buffer = [0u8; 4096];
            client_stream
                .read(&mut response_buffer)
                .await
                .expect("Failed to read server response");

            // TODO: Parse the server's response if necessary and determine if an acknowledgment or other message should be sent.

            // Send acknowledgment or other message
            let ack_data: &[u8] = &[66, 0, 0, 0, 0, 0, 4, 3, 0, 0, 12, 35]; // Your mock acknowledgment data
            client_stream
                .write_all(ack_data)
                .await
                .expect("Failed to send acknowledgment");
        });

        // Server handling
        let (mut server_stream, _) = listener
            .accept()
            .await
            .expect("Failed to accept connection");
        let mut conn = Connection::new(server_stream);
        let rtmp_message = conn.read_message().await.expect("Failed to read message");

        if let RtmpMessage::Connect(connect_msg) = rtmp_message {
            conn.handle_connect(connect_msg)
                .await
                .expect("Failed to handle connect message");
        } else {
            println!("Received message: {:?}", rtmp_message);
            panic!("Expected a ConnectMessage but received a different type");
        }

        // Add any further assertions or verifications here
    }
}
