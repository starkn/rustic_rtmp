// This file will handle the RTMP protocol. It should define functions for handling the handshake, chunking and de-chunking messages, and processing different types of messages.

// Path: src/protocol.rs
use crate::server::connection::message::message::{AcknowledgementMessage, AudioData, BasicCommand, CommandObject, ConnectMessage, CreateStream, Event, FCPublish, OnStatus, Publish, ReleaseStream, ResultObject, RtmpMessage, SetChunkSizeMessage, SetDataFrame, VideoData};
use crate::server::connection::message::define::msg_type_id;


use log::{error, info, warn};
use rand::{Rng, SeedableRng};
use std::error;

const WINDOW_ACKNOWLEDGEMENT_SIZE: u32 = 4096;
const SET_BANDWIDTH_SIZE: u32 = 4096;
pub const HANDSHAKE_CHUNK_SIZE: usize = 1536;
pub const DEFAULT_CHUNK_SIZE: usize = 128;


fn insert_bytes(dst: &mut [u8; 12], data: u32, start_idx: usize, end_idx: usize) {
    let data_as_bytes = data.to_be_bytes();

    let mut i: usize = 0;
    if end_idx - start_idx == 3 {
        i = 1;
    }

    for idx in start_idx..end_idx {
        dst[idx] = data_as_bytes[i];
        i += 1;
    }
}

fn write_header(
    msg_type_id: u8,
    msg_len: u32,
    timestamp: u32,
    stream_id: u32,
    chunk_basic_header: u8,
) -> [u8; 12] {

    let mut header = [0; 12];
    header[0] = chunk_basic_header;
    insert_bytes(&mut header, timestamp, 1, 4);
    insert_bytes(&mut header, msg_len, 4, 7);
    header[7] = msg_type_id;
    insert_bytes(&mut header, stream_id, 8, 12);

    info!("header: {:?}", header);
    header
}

pub fn read_header(data: &Vec<u8>, header: ChunkBasicHeader, chunk: ChunkMessageHeader) -> Result<ChunkMessageHeader, Box<dyn error::Error>> {
    match ChunkFmt::from_u8(header.fmt)
    {
        Some(ChunkFmt::Type0) =>
            {
                let chunk_message_header = ChunkMessageHeader::type0(&data);
                return Ok(chunk_message_header);
            }
        Some(ChunkFmt::Type1) =>
            {
                let mut chunk_message_header = ChunkMessageHeader::type1(&data);
                chunk_message_header.timestamp = chunk.timestamp;
                chunk_message_header.message_stream_id = chunk.message_stream_id;
                return Ok(chunk_message_header);
            }
        Some(ChunkFmt::Type2) =>
            {
                let mut chunk_message_header = ChunkMessageHeader::type2(&data);
                chunk_message_header.timestamp = chunk.timestamp;
                chunk_message_header.message_length = chunk.stored_message_length;
                chunk_message_header.message_type_id = chunk.message_type_id;
                chunk_message_header.message_stream_id = chunk.message_stream_id;
                return Ok(chunk_message_header);
            }
        Some(ChunkFmt::Type3) => {
            return Ok(chunk);
        }
        _ => {
            info!("Unknown chunk format");
        }
    }
    error!("Unknown chunk format");
    Err("Unknown chunk format".into())
}

pub fn header_size_from_type(header: &ChunkBasicHeader) -> Result<usize, Box<dyn error::Error>> {
    match ChunkFmt::from_u8(header.fmt)
    {
        Some(ChunkFmt::Type0) => return Ok(11usize),
        Some(ChunkFmt::Type1) => return Ok(7usize),
        Some(ChunkFmt::Type2) => return Ok(3usize),
        Some(ChunkFmt::Type3) => return Ok(0usize),
        _ => info!("Unknown chunk format")
    }
    error!("Unknown chunk format");
    Err("Unknown chunk format".into())
}

pub fn parse_message(chunk: ChunkMessageHeader, data: Vec<u8>) -> Result<RtmpMessage, Box<dyn error::Error>> {
    match chunk.message_type_id {
        Some(msg_type_id::SET_CHUNK_SIZE) => {
            info!("Message type: Set Chunk Size");
            let tmp_data = (data[0] << 1) >> 1;
            let chunk_size = u32::from_be_bytes([tmp_data, data[1], data[2], data[3]]) as usize;
            info!("chunk_size: {}", chunk_size);

            let set_chunk_size = SetChunkSizeMessage::new(chunk_size);
            return Ok(RtmpMessage::SetChunkSize(set_chunk_size));
        }
        Some(msg_type_id::ABORT) => {
            info!("Message type: Abort");
        }
        Some(msg_type_id::ACKNOWLEDGEMENT) => {
            info!("Message type: Acknowledgement");
            let tmp_data = (data[0] << 1) >> 1;
            let ack = u32::from_be_bytes([tmp_data, data[1], data[2], data[3]]);
            let ack = AcknowledgementMessage::new(ack);
            info!("ack: {:?}", ack);
            return Ok(RtmpMessage::Acknowledgement(ack));
        }
        Some(msg_type_id::USER_CONTROL_EVENT) => {
            info!("Message type: User Control");
        }
        Some(msg_type_id::WIN_ACKNOWLEDGEMENT_SIZE) => {
            info!("Message type: Window Acknowledgement Size");
        }
        Some(msg_type_id::SET_PEER_BANDWIDTH) => {
            info!("Message type: Set Peer Bandwidth");
        }
        Some(msg_type_id::AUDIO) => {
            info!("Message type: Audio");
            let audio_data = AudioData::new(chunk.message_stream_id.unwrap(), data);
            return Ok(RtmpMessage::AudioData(audio_data));
        }
        Some(msg_type_id::VIDEO) => {
            info!("Message type: Video");
            let video_data = VideoData::new(chunk.message_stream_id.unwrap(), data);
            return Ok(RtmpMessage::VideoData(video_data));
        }
        Some(msg_type_id::COMMAND_AMF3) => {
            info!("Message type: Command AMF3");
        }
        Some(msg_type_id::DATA_AMF3) => {
            info!("Message type: Data AMF3");
        }
        Some(msg_type_id::SHARED_OBJ_AMF3) => {
            info!("Message type: Shared Object AMF3");
        }
        Some(msg_type_id::DATA_AMF0) => {
            info!("Message type: Data AMF0");
            let msg_name = BasicCommand::parse(&data)?.command_name;
            info!("msg_name: {:?}", msg_name);
            return match msg_name.as_str() {
                "@setDataFrame" => {
                    let message = SetDataFrame::parse(&data)?;
                    info!("message: {:?}", message);
                    Ok(RtmpMessage::SetDataFrame(message))
                }
                _ => {
                    error!("Unknown Data: {:?}", msg_name);
                    Err("Unknown Data".into())
                }
            }
        }
        Some(msg_type_id::SHARED_OBJ_AMF0) => {
            info!("Message type: Shared Object AMF0");
        }
        Some(msg_type_id::AGGREGATE) => {
            info!("Message type: Aggregate");
        }
        Some(msg_type_id::COMMAND_AMF0) => {
            info!("Message type: Command AMF0");
            let command_name = BasicCommand::parse(&data)?.command_name;
            info!("command_name: {:?}", command_name);
            return match command_name.as_str() {
                "connect" => {
                    let message = ConnectMessage::parse(&data)?;
                    Ok(RtmpMessage::Connect(message))
                }
                "releaseStream" => {
                    let message = ReleaseStream::parse(&data)?;
                    info!("releaseStream: {:?}", message);
                    Ok(RtmpMessage::ReleaseStream(message))
                }
                "FCPublish" => {
                    let message = FCPublish::parse(&data)?;
                    info!("FCPublish: {:?}", message);
                    Ok(RtmpMessage::FCPublish(message))
                }
                "createStream" => {
                    let message = CreateStream::parse(&data)?;
                    info!("createStream: {:?}", message);
                    Ok(RtmpMessage::CreateStream(message))
                }
                "publish" => {
                    let message = Publish::parse(&data)?;
                    info!("publish: {:?}", message);
                    Ok(RtmpMessage::Publish(message))
                }
                _ => {
                    error!("Unknown command: {:?}", command_name);
                    Err("Unknown command".into())
                }
            };
        }
        _ => {
            error!("Message type: Unknown {:?}", chunk.message_type_id);
            return Err("Unknown message type".into());
        }
    }
    error!("Unknown message type");
    Err("Unknown message type".into())
}

pub fn s_chunk(c0: [u8; 1], c1: [u8; HANDSHAKE_CHUNK_SIZE])
               -> Result<([u8; 1],[u8; HANDSHAKE_CHUNK_SIZE],[u8; HANDSHAKE_CHUNK_SIZE]), Box<dyn error::Error>> {
    // Check the RTMP version in C0.
    let version = c0[0];
    if version != 3 {
        error!("Unsupported RTMP version: {}", version);
        return Err("Unsupported RTMP version".into());
    }

    // Check the timestamp in C1.
    let _timestamp = u32::from_be_bytes([
        c1[0],
        c1[1],
        c1[2],
        c1[3],
    ]);

    let mut s0 = [0; 1];
    let mut s1 = [0; HANDSHAKE_CHUNK_SIZE];
    let mut s2 = [0; HANDSHAKE_CHUNK_SIZE];

    // Construct S0.
    s0[0] = version; // RTMP version

    // Construct S1.
    let server_timestamp = 0u32.to_be_bytes(); // Server uptime in milliseconds
    s1[0..4].copy_from_slice(&server_timestamp);

    let mut rng = rand::rngs::StdRng::from_entropy();
    rng.fill(&mut s1[4..]);

    // Construct S2 by copying C1.
    s2.copy_from_slice(&c1);
    Ok((s0, s1, s2))
}

pub fn ack_msg() -> Result<[u8; 16], Box<dyn error::Error>> {
    let ack_header = write_header(5, 4, 0, 0, 2);
    let mut ack_msg: [u8; 16] = [0; 16];

    ack_msg[0..12].copy_from_slice(&ack_header);
    ack_msg[12..16].copy_from_slice(&WINDOW_ACKNOWLEDGEMENT_SIZE.to_be_bytes());

    Ok(ack_msg)
}

pub fn band_msg() -> Result<Vec<u8>, Box<dyn error::Error>> {
    let bandwidth_header = write_header(6, 5, 0, 0, 2);
    let mut bandwidth_msg: [u8; 17] = [0; 17];

    bandwidth_msg[0..12].copy_from_slice(&bandwidth_header);

    let bandwidth_as_bytes = SET_BANDWIDTH_SIZE.to_be_bytes();

    bandwidth_msg[12..16].copy_from_slice(&bandwidth_as_bytes);
    bandwidth_msg[16] = 2;
    let e = CommandObject::new("FMS/3,0,1,123".to_string(), 31);
    let mut result_obj = ResultObject::new("_result".to_string(), 1, 0);
    result_obj.set_command_object(e);
    let command = result_obj.parse()?;
    let command_vec: Vec<u8> = command.freeze().to_vec();

    let command_header = write_header(20, command_vec.len() as u32, 0, 0, 2);
    let mut command_msg = Vec::new();
    command_msg.extend_from_slice(&command_header);
    command_msg.extend_from_slice(&command_vec);

    let mut set_peer_bandwidth = Vec::new();
    set_peer_bandwidth.extend_from_slice(&bandwidth_msg);
    set_peer_bandwidth.extend_from_slice(&command_msg);
    info!("set peer bandwidth: {:?}", set_peer_bandwidth);

    Ok(set_peer_bandwidth)
}

pub fn result_msg(msg: CreateStream) -> Result<Vec<u8>, Box<dyn error::Error>> {
    let result_obj = ResultObject::new("_result".to_string(), msg.transaction_id, 1);
    let command = result_obj.parse()?;
    let command_vec: Vec<u8> = command.freeze().to_vec();
    let result_header = write_header(20, command_vec.len() as u32, 0, 0, 3);
    let mut result_msg = Vec::new();
    result_msg.extend_from_slice(&result_header);
    result_msg.extend_from_slice(&command_vec);
    info!("result msg: {:?}", result_msg);

    Ok(result_msg)
}

pub fn stream_begin_msg() -> Result<Vec<u8>, Box<dyn error::Error>> {
    let stream_begin_header = write_header(4, 6, 0, 0, 2);
    let stream_begin = Event::new(0, 1).parse();
    let mut stream_begin_msg = Vec::new();
    stream_begin_msg.extend_from_slice(&stream_begin_header);
    stream_begin_msg.extend_from_slice(&stream_begin);
    info!("stream begin msg: {:?}", stream_begin_msg);

    Ok(stream_begin_msg)
}

pub fn on_status_msg(msg: Publish) -> Result<Vec<u8>, Box<dyn error::Error>> {
    let on_status = OnStatus::new(msg.transaction_id).parse();
    let mut on_status_vec: Vec<u8> = Vec::new();
    on_status_vec.extend_from_slice(&on_status.unwrap());
    let on_status_header = write_header(20, on_status_vec.len() as u32, 0, 1, 3);
    let mut on_status_msg = Vec::new();
    on_status_msg.extend_from_slice(&on_status_header);
    on_status_msg.extend_from_slice(&on_status_vec);
    info!("on status msg: {:?}", on_status_msg);

    Ok(on_status_msg)
}

enum ChunkFmt {
    Type0,
    Type1,
    Type2,
    Type3,
}

impl ChunkFmt {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Type0),
            1 => Some(Self::Type1),
            2 => Some(Self::Type2),
            3 => Some(Self::Type3),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct ChunkBasicHeader {
    pub fmt: u8,
    pub cs: u8,
}

impl ChunkBasicHeader {
    pub fn new(byte: &u8) -> ChunkBasicHeader {
        // split into the chunk header and the message body
        let cs = byte & 0b_00111111;
        let fmt = (byte >> 6) & 0b_00000011;

        ChunkBasicHeader { fmt, cs }
    }
}

#[derive(Copy, Clone)]
pub struct ChunkMessageHeader {
    pub timestamp: Option<u32>,
    pub timestamp_delta: Option<u32>,
    pub message_length: Option<u32>,
    pub stored_message_length: Option<u32>,
    pub message_type_id: Option<u8>,
    pub message_stream_id: Option<u32>,
}

impl ChunkMessageHeader {
    pub fn default() -> ChunkMessageHeader {
        ChunkMessageHeader {
            timestamp: None,
            timestamp_delta: None,
            message_length: None,
            stored_message_length: None,
            message_type_id: None,
            message_stream_id: None,
        }
    }

    pub fn type0(bytes: &[u8]) -> ChunkMessageHeader {
        let mut chunk_message_header = ChunkMessageHeader::default();

        let timestamp = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
        let message_length = u32::from_be_bytes([0, bytes[3], bytes[4], bytes[5]]); // maybe switch 0 to end of arguments?
        let message_type_id = bytes[6];
        let message_stream_id = u32::from_be_bytes([bytes[10], bytes[9], bytes[8], bytes[7]]);

        chunk_message_header.timestamp = Some(timestamp);
        chunk_message_header.message_length = Some(message_length);
        chunk_message_header.stored_message_length = Some(message_length);
        chunk_message_header.message_type_id = Some(message_type_id);
        chunk_message_header.message_stream_id = Some(message_stream_id);

        info!("timestamp: {}", timestamp);
        info!("message_length: {}", message_length);
        info!("message_type_id: {}", message_type_id);
        info!("message_stream_id: {}", message_stream_id);

        chunk_message_header
    }

    pub fn type1(bytes: &[u8]) -> ChunkMessageHeader {
        let mut chunk_message_header = ChunkMessageHeader::default();

        let timestamp_delta = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
        let message_length = u32::from_be_bytes([0, bytes[3], bytes[4], bytes[5]]);
        let message_type_id = bytes[6];

        chunk_message_header.timestamp_delta = Some(timestamp_delta);
        chunk_message_header.message_length = Some(message_length);
        chunk_message_header.stored_message_length = Some(message_length);
        chunk_message_header.message_type_id = Some(message_type_id);

        info!("timestamp_delta: {}", timestamp_delta);
        info!("message_length: {}", message_length);
        info!("message_type_id: {}", message_type_id);

        chunk_message_header
    }

    pub fn type2(bytes: &[u8]) -> ChunkMessageHeader {
        let mut chunk_message_header = ChunkMessageHeader::default();

        chunk_message_header.timestamp_delta =
            Some(u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]));

        chunk_message_header
    }

    pub fn type3() -> ChunkMessageHeader {
        ChunkMessageHeader::default()
    }
}