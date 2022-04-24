use anyhow::Result;
use webrtc::api::media_engine::{MIME_TYPE_VP8, MIME_TYPE_OPUS};
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtp_transceiver::rtp_receiver::RTCRtpReceiver;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};
use webrtc::track::track_remote::TrackRemote;
use flume::Sender;
use flume::Receiver;
use std::collections::HashMap;
use crate::sfu::signal::SocketMessage;
use crate::PeerChanCommand;

#[derive(Debug, Clone)]
pub struct Peer {
    // The peer connection itself
    pub pc: Arc<RTCPeerConnection>,
    // Copy of the socket to transmit back on
    pub tx: Sender<SocketMessage>,
    pub output_tracks: HashMap<String, Arc<TrackLocalStaticRTP>>,
    // The id for this peer in the call
    pub uuid: String,
}

// This is ran in a tokio task, that holds all the shared state. It's communicated to by channels.
pub async fn handle_peer_connection_commands(peer_chan_rx: Receiver<PeerChanCommand>, peer_chan_tx: Sender<PeerChanCommand>) -> Result<()> {
    let api = crate::sfu::api::prepare_api()?;

    let mut peers: HashMap<String, Peer> = HashMap::new();
    // let mut output_tracks: HashMap<String, Arc<TrackLocalStaticRTP>> = HashMap::new();
    // let mut input_tracks: HashMap<String, Arc<TrackRemote>> = HashMap::new();

    while let Ok(cmd) = peer_chan_rx.recv_async().await {
        use PeerChanCommand::*;

        println!("👻👻👻👻");
        match cmd {
            SendIceCandidate { uuid, candidate } => {
                let peer = peers.get(&uuid).unwrap();
                let pc = Arc::clone(&peer.pc);

                peer.tx.send(SocketMessage {
                    event: String::from("candidate"),
                    data: candidate,
                    uuid: uuid.to_owned()
                }).unwrap();
            }
            ReceiveIceCandidate { uuid, candidate } => {
                println!("\nReceived Ice candidate.\n");
                thread::sleep(Duration::from_millis(200));
                let peer = peers.get(&uuid).unwrap();
                let pc = Arc::clone(&peer.pc);
                let can: RTCIceCandidateInit = serde_json::from_str(&candidate).unwrap();
                pc.add_ice_candidate(can).await.unwrap();
            }
            SendOffer { uuid } => {
                println!("👀 Renegotiating for {}...", uuid);
                let peer = peers.get_mut(&uuid).unwrap();
                let pc = Arc::clone(&peer.pc);

                let offer = pc.create_offer(None).await?;
                let offer_string = serde_json::to_string(&offer)?;

                pc.set_local_description(offer).await.unwrap();

                peer.tx.send(SocketMessage {
                    event: String::from("offer"),
                    data: offer_string,
                    uuid: uuid.to_owned()
                }).unwrap();
            }
            ReceiveOffer { uuid, sdp, tx } => {
                let tx_clone = tx.clone();
                match peers.get(&uuid) {
                    Some(peer) => {
                        let pc = Arc::clone(&peer.pc);
                        let offer = RTCSessionDescription::offer(sdp).unwrap();
                        pc.set_remote_description(offer).await.unwrap();
                    }
                    None => {
                        let config = crate::sfu::api::prepare_configuration()?;

                        // Step 1: Make a output_track for every other peer
                        let mut peer = Peer {
                            pc: Arc::new(api.new_peer_connection(config).await?),
                            uuid: uuid.clone(),
                            output_tracks: HashMap::new(),
                            tx,
                        };
                        let pc = Arc::clone(&peer.pc);

                        let offer = RTCSessionDescription::offer(sdp).unwrap();
                        pc.set_remote_description(offer).await.unwrap();

                        let mut media = vec![];
                        media.push("audio");
                        media.push("video");

                        // Step 2: Add an tracks to this peer, for every other peer
                        for (_key, p) in &peers {
                            for s in media.clone() {
                                let output_track = Arc::new(TrackLocalStaticRTP::new(
                                        RTCRtpCodecCapability {
                                            mime_type: if s == "video" {
                                                MIME_TYPE_VP8.to_owned()
                                            } else {
                                                MIME_TYPE_OPUS.to_owned()
                                            },
                                            ..Default::default()
                                        },
                                        format!("track-{}", s),
                                        "webrtc-rs".to_owned(),
                                ));

                                let rtp_sender = pc
                                    .add_track(Arc::clone(&output_track) as Arc<dyn TrackLocal + Send + Sync>)
                                    .await?;

                                let m = s.to_owned();
                                tokio::spawn(async move {
                                    let mut rtcp_buf = vec![0u8; 1500];
                                    while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
                                    println!("{} rtp_sender.read loop exit", m);
                                    Result::<()>::Ok(())
                                });

                                peer.output_tracks.insert(s.to_owned(), output_track);
                            }
                        }

                        // Step 3: Add 1 track to every other peer
                        add_track_to_other_peers(&mut peers, uuid.to_owned()).await.unwrap();

                        set_pc_callbacks(&mut peer, peer_chan_tx.clone()).await.unwrap();

                        let answer = pc.create_answer(None).await?;
                        let answer_string = serde_json::to_string(&answer)?;

                        pc.set_local_description(answer).await.unwrap();

                        tx_clone.send(SocketMessage {
                            event: String::from("answer"),
                            data: answer_string.to_owned(),
                            uuid: uuid.to_owned()
                        }).unwrap();

                        peers.insert(uuid.to_owned(), peer);
                    }
                }
            }
            ReceiveAnswer { uuid, sdp } => {
                let peer = peers.get(&uuid).unwrap();
                let pc = Arc::clone(&peer.pc);

                let answer = RTCSessionDescription::answer(sdp).unwrap();
                pc.set_remote_description(answer).await.unwrap();
            },
            OnTrack { uuid, track } => {
                for (key, peer) in &peers {
                    let t = track.clone();

                    let mut output_tracks = &peer.output_tracks;

                    let kind = if t.kind() == RTPCodecType::Audio {
                        "audio"
                    } else {
                        "video"
                    };
                    let output_track = if let Some(output_track) = output_tracks.get(kind) {
                        Arc::clone(output_track);

                        let output_track2 = Arc::clone(&output_track);
                        tokio::spawn(async move {
                            println!(
                                "Track has started, of type {}: {}",
                                t.payload_type(),
                                t.codec().await.capability.mime_type
                            );
                            // Read RTP packets being sent to webrtc-rs
                            while let Ok((rtp, _)) = t.read_rtp().await {
                                if let Err(err) = output_track2.write_rtp(&rtp).await {
                                    println!("output track write_rtp got error: {}", err);
                                    break;
                                }
                            }

                            println!(
                                "on_track finished, of type {}: {}",
                                t.payload_type(),
                                t.codec().await.capability.mime_type
                            );
                        });
                    };
                }
            },
        }
    }

    Ok(())
}

async fn add_track_to_other_peers(peers: &mut HashMap<String, Peer>, uuid: String) -> anyhow::Result<()> {
    let mut media = vec![];
    media.push("audio");
    media.push("video");

    for (key, p) in peers {
        if key == uuid.as_str() {
            continue;
        } else {
            for s in media.clone() {
                let output_track = Arc::new(TrackLocalStaticRTP::new(
                        RTCRtpCodecCapability {
                            mime_type: if s == "video" {
                                MIME_TYPE_VP8.to_owned()
                            } else {
                                MIME_TYPE_OPUS.to_owned()
                            },
                            ..Default::default()
                        },
                        format!("track-{}", s),
                        "webrtc-rs".to_owned(),
                ));

                let rtp_sender = p.pc
                    .add_track(Arc::clone(&output_track) as Arc<dyn TrackLocal + Send + Sync>)
                    .await?;

                let m = s.to_owned();
                tokio::spawn(async move {
                    let mut rtcp_buf = vec![0u8; 1500];
                    while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
                    println!("{} rtp_sender.read loop exit", m);
                    Result::<()>::Ok(())
                });

                p.output_tracks.insert(s.to_owned(), output_track);
            }
        }
    }

    Ok(())
}

async fn set_pc_callbacks(peer: &mut Peer, peer_chan_tx: Sender<PeerChanCommand>) -> anyhow::Result<()> {
    // Set the handler for when renegotiation needs to happen
    let mut tx_clone = peer_chan_tx.clone();
    let mut uuid = peer.uuid.clone();
    peer.pc
        .on_negotiation_needed(Box::new(move || {
            println!("🔥 Peer Connection needs negotiation.");
            let cloned_tx = tx_clone.clone();
            let cloned_id = uuid.clone();

            Box::pin(async move {
                cloned_tx.send(PeerChanCommand::SendOffer {
                    uuid: cloned_id.to_owned(),
                }).unwrap();
            })
        }))
    .await;

    // Set the hendler for receiving a new trickle ice candidate.
    let mut tx_clone = peer_chan_tx.clone();
    let mut uuid = peer.uuid.clone();
    peer.pc
        .on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
            let cloned_tx = tx_clone.clone();
            let cloned_id = uuid.clone();

            Box::pin(async move {
                if !candidate.is_none() {
                    cloned_tx.send(PeerChanCommand::SendIceCandidate {
                        uuid: cloned_id.to_owned(),
                        candidate: serde_json::to_string(&candidate.unwrap()).unwrap(),
                    }).unwrap();
                }
            })
        })).await;

    // Set a handler for when a new remote track starts, this handler copies inbound RTP packets,
    // replaces the SSRC and sends them back
    let pc = Arc::downgrade(&peer.pc);
    let mut tx_clone = peer_chan_tx.clone();
    uuid = peer.uuid.clone();
    peer.pc
        .on_track(Box::new(
                move |track: Option<Arc<TrackRemote>>, _receiver: Option<Arc<RTCRtpReceiver>>| {
                    if let Some(track) = track {
                        // Send a PLI on an interval so that the publisher is pushing a keyframe every rtcpPLIInterval
                        // This is a temporary fix until we implement incoming RTCP events, then we would push a PLI only when a viewer requests it
                        let media_ssrc = track.ssrc();

                        if track.kind() == RTPCodecType::Video {
                            let pc2 = pc.clone();
                            tokio::spawn(async move {
                                let mut result = Result::<usize>::Ok(0);
                                while result.is_ok() {
                                    let timeout = tokio::time::sleep(Duration::from_secs(3));
                                    tokio::pin!(timeout);

                                    tokio::select! {
                                        _ = timeout.as_mut() =>{
                                            if let Some(pc) = pc2.upgrade(){
                                                result = pc.write_rtcp(&[Box::new(PictureLossIndication{
                                                    sender_ssrc: 0,
                                                    media_ssrc,
                                                })]).await.map_err(Into::into);
                                            }else{
                                                break;
                                            }
                                        }
                                    };
                                }
                            });
                        }

                        tx_clone.send(PeerChanCommand::OnTrack {
                            uuid: uuid.to_owned(),
                            track
                        }).unwrap();
                    }
                    Box::pin(async {})
                },
    )).await;

    peer.pc
        .on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
            println!("Peer Connection State has changed: {}", s);
            Box::pin(async {})
        })).await;
    Ok(())
}
