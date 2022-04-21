use anyhow::Result;
use sfu::signal::SocketMessage;
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use std::sync::Arc;
use tokio::time::Duration;
use webrtc::api::API;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_VP8};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtp_transceiver::rtp_codec::{RTPCodecType, RTCRtpCodecCapability};
use webrtc::rtp_transceiver::rtp_receiver::RTCRtpReceiver;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};
use webrtc::track::track_remote::TrackRemote;
use webrtc::Error;
use flume::Sender;
use std::collections::HashMap;

mod sfu;

#[derive(Debug, Clone)]
enum PeerChanCommand {
    NewOffer {
        uuid: String,
    },
    AddIceCandidate {
        uuid: String,
        candidate: String,
    },
    ReceiveAnswer {
        uuid: String,
        sdp: String,
    },
    AddPeer {
        uuid: String,
        pc: Arc<RTCPeerConnection>,
        tx: Sender<SocketMessage>
    },
    AddTrack {
        uuid: String,
        track: Arc<TrackLocalStaticRTP>
    }
}

#[derive(Debug, Clone)]
pub struct Peer {
    pub pc: Arc<RTCPeerConnection>,
    pub tx: Sender<SocketMessage>,
    pub uuid: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let new_conn_rx = sfu::signal::ws_sdp_signaler(8081).await;

    let (peer_chan_tx, peer_chan_rx) = flume::unbounded::<PeerChanCommand>();

    let peer_chan_inner_tx = peer_chan_tx.clone();
    tokio::spawn(async move {
        // let mut pcs: HashMap<String, Arc<RTCPeerConnection>> = HashMap::new();
        let mut pcs: HashMap<String, Peer> = HashMap::new();

        while let Ok(cmd) = peer_chan_rx.recv_async().await {
            use PeerChanCommand::*;

            println!("👻👻👻👻");
            match cmd {
                NewOffer { uuid } => {
                    if let Some(peer) = pcs.get(&uuid) {
                        let clone = Arc::clone(&peer.pc);

                        let offer: RTCSessionDescription = clone.create_offer(None).await.unwrap();
                        let offer_string = serde_json::to_string(&offer).unwrap();
                        clone.set_local_description(offer).await.unwrap();

                        println!("");
                        println!("whats this peer look like");
                        for receiver in clone.get_receivers().await {
                            println!("{:?}", receiver);
                        }
                        println!("");

                        // tx.send(sfu::signal::SocketMessage {
                        //     event: String::from("offer"),
                        //     data: offer_string,
                        //     uuid: uuid.to_owned()
                        // }).unwrap();
                    }
                }
                AddIceCandidate { uuid, candidate } => {
                    let peer = pcs.get(&uuid).unwrap();
                    let clone = Arc::clone(&peer.pc);

                    let can: RTCIceCandidateInit = serde_json::from_str(&candidate).unwrap();
                    println!("candidate: {:?}", can);
                    clone.add_ice_candidate(can).await.unwrap();
                }
                ReceiveAnswer { uuid, sdp } => {
                    let peer = pcs.get(&uuid).unwrap();
                    let clone = Arc::clone(&peer.pc);

                    println!("state: {:?}", peer.pc.signaling_state());

                    let answer = RTCSessionDescription::answer(sdp).unwrap();
                    clone
                        .set_remote_description(answer)
                        .await
                        .unwrap();
                },
                AddPeer { uuid, pc, tx } => {
                    println!("Got new pc in manager!");
                    let clone = Arc::clone(&pc);
                    let id = uuid.clone();
                    pcs.insert(uuid, Peer {
                        pc: clone,
                        uuid: id,
                        tx
                    });
                },
                AddTrack { uuid, track } => {
                    for (key, peer) in &pcs {
                        let pc = &peer.pc;
                        if key == uuid.as_str() {
                            continue;
                        } else {
                            println!("Adding track to this pc.");
                            let _rtp_sender = pc
                                .add_track(Arc::clone(&track) as Arc<dyn TrackLocal + Send + Sync>)
                                .await
                                .unwrap();

                            let clone_of_tx = peer_chan_inner_tx.clone();
                            peer_chan_inner_tx.send(PeerChanCommand::NewOffer {
                                uuid: uuid.to_owned()
                            }).unwrap();
                        }
                    }
                },
            }
        }
    });

    while let Ok((uuid, socket_tx, socket_rx)) = new_conn_rx.recv_async().await {
        // To simplify things, create the peer immediately upon a new connection, and send them an
        // offer. This way the SFU only takes in answers, and sends out offers. Guess that doesn't
        // work if they change tracks but it's a start.
        let cloned_peer_chan_tx = peer_chan_tx.clone();
        let cloned_peer_chan_2_tx = peer_chan_tx.clone();
        let cloned_again_peer_chan_tx = cloned_peer_chan_tx.clone();
        let (peer_connection, id) = create_peer(uuid.to_owned(), cloned_peer_chan_tx).await.unwrap();

        let cloned_id_again = uuid.clone();
        let cloned_id_2 = uuid.clone();
        let cloned_id_3 = uuid.clone();
        let cloned_socket_tx = socket_tx.clone();
        let cloned_socket_again_tx = socket_tx.clone();
        peer_connection
            .on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
                let cloned_2_socket_tx = socket_tx.clone();
                let cloned_id = uuid.clone();
                Box::pin(async move {
                    // if candidate.is_none() {
                        // let _ = cloned_4_peer_chan_tx.send(()).await;
                        cloned_2_socket_tx.send(sfu::signal::SocketMessage {
                            event: String::from("candidate"),
                            data: serde_json::to_string(&candidate).unwrap(),
                            uuid: cloned_id,
                        }).unwrap();
                 // }
                })
            })).await;

        // Set the handler for when renegotiation needs to happen
        peer_connection
            .on_negotiation_needed(Box::new(move || {
                println!("Peer Connection needs negotiation.");
                // let cloned_2_socket_tx = socket_tx.clone();

                cloned_peer_chan_2_tx.send(PeerChanCommand::NewOffer {
                    uuid: cloned_id_3.to_owned(),
                }).unwrap();

                Box::pin(async {})
            }))
        .await;

        let track_a = Arc::new(TrackLocalStaticSample::new(
                RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_VP8.to_owned(),
                    ..Default::default()
                },
                "video".to_owned(),
                "webrtc-rs".to_owned(),
        ));

        let _rtp_sender = peer_connection
            .add_track(Arc::clone(&track_a) as Arc<dyn TrackLocal + Send + Sync>)
            .await
            .unwrap();

        let offer = peer_connection.create_offer(None).await?;
        let offer_string = serde_json::to_string(&offer)?;

        // Set local description
        peer_connection.set_local_description(offer).await?;

        // Allow us to receive 1 video track
        peer_connection
            .add_transceiver_from_kind(RTPCodecType::Video, &[])
            .await?;

        println!("Sending msg out");
        cloned_socket_tx.send(sfu::signal::SocketMessage {
            event: String::from("offer"),
            data: offer_string,
            uuid: cloned_id_again
        }).unwrap();

        peer_chan_tx.send(PeerChanCommand::AddPeer {
            uuid: id.to_owned(),
            pc: peer_connection,
            tx: cloned_socket_again_tx.clone()
        }).unwrap();

        tokio::spawn(async move {
            while let Ok(signal) = socket_rx.recv_async().await {
                match signal {
                    sfu::signal::SocketMessage { event, uuid: id, data: sdp } if event == "answer" => {
                        println!("Got an answer back");
                        cloned_again_peer_chan_tx.send(PeerChanCommand::ReceiveAnswer {
                            uuid: id.to_owned(),
                            sdp
                        }).unwrap();
                    },
                    sfu::signal::SocketMessage { event, uuid: id, data: candidate } if event == "candidate" => {
                        cloned_again_peer_chan_tx.send(PeerChanCommand::AddIceCandidate {
                            uuid: id.to_owned(),
                            candidate
                        }).unwrap();
                    },
                    _ => ()
                }
            };
        });
    };

    Ok(())
}

async fn create_peer(uuid: String, peer_chan_tx: Sender<PeerChanCommand>) -> Result<(Arc<RTCPeerConnection>, String), anyhow::Error> {

    let api = prepare_api()?;

    let config = prepare_configuration()?;

    println!("Creating the local peer");

    // Create a new local RTCPeerConnection
    let peer_connection = Arc::new(api.new_peer_connection(config).await?);

    // Set a handler for when a new remote track starts, this handler copies inbound RTP packets,
    // replaces the SSRC and sends them back
    let pc = Arc::downgrade(&peer_connection);
    let id = uuid.to_owned();
    peer_connection
        .on_track(Box::new(
                move |track: Option<Arc<TrackRemote>>, _receiver: Option<Arc<RTCRtpReceiver>>| {
                    println!("✨✨✨✨✨✨✨✨✨✨✨✨✨✨✨✨✨✨✨✨✨✨✨✨✨✨✨✨ Received a new track!!!");

                    if let Some(track) = track {
                        // Send a PLI on an interval so that the publisher is pushing a keyframe every rtcpPLIInterval
                        // This is a temporary fix until we implement incoming RTCP events, then we would push a PLI only when a viewer requests it
                        let media_ssrc = track.ssrc();
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
                                        } else {
                                            break;
                                        }
                                    }
                                };
                            }
                        });

                        let local_track_chan_tx = peer_chan_tx.clone();
                        let mut id = id.to_owned();
                        tokio::spawn(async move {
                            // Create Track that we send video back to browser on
                            let local_track = Arc::new(TrackLocalStaticRTP::new(
                                    track.codec().await.capability,
                                    "video".to_owned(),
                                    "webrtc-rs".to_owned(),
                            ));

                            println!("");
                            println!("Adding a new track.");
                            println!("");
                            local_track_chan_tx.send(
                                PeerChanCommand::AddTrack {
                                    uuid: id,
                                    track: Arc::clone(&local_track)
                                }
                            ).unwrap();

                            // Read RTP packets being sent to webrtc-rs
                            while let Ok((rtp, _)) = track.read_rtp().await {
                                if let Err(err) = local_track.write_rtp(&rtp).await {
                                    println!("wrote packet");
                                    if Error::ErrClosedPipe != err {
                                        print!("output track write_rtp got error: {} and break", err);
                                        break;
                                    } else {
                                        print!("output track write_rtp got error: {}", err);
                                    }
                                }
                            }
                        });
                    }

                    Box::pin(async {})
                },
    ))
        .await;

    handle_pc(&peer_connection).await.unwrap();

    Ok((peer_connection, uuid.to_owned()))
}

async fn handle_pc(peer_connection: &Arc<RTCPeerConnection>) -> Result<(), anyhow::Error> {
    // Set the handler for Peer connection state
    // This will notify you when the peer has connected/disconnected
    peer_connection
        .on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
            println!("Peer Connection State has changed: {}", s);
            Box::pin(async {})
        }))
    .await;

    println!("are we 1");

    // // Set the remote SessionDescription
    // peer_connection
    //     .set_remote_description(offer)
    //     .await?;

    // // Create an answer
    // let answer = peer_connection.create_answer(None).await?;

    // Create channel that is blocked until ICE Gathering is complete
    // let mut gather_complete = peer_connection.gathering_complete_promise().await;

    println!("are we 2");
    // // Sets the LocalDescription, and starts our UDP listeners
    // peer_connection.set_local_description(answer).await?;

    // Block until ICE Gathering is complete, disabling trickle ICE
    // we do this because we only can exchange one signaling message
    // in a production application you should exchange ICE Candidates via OnICECandidate
    // TODO: have to do trickle ice I guess
    // let _ = gather_complete.recv().await;

    println!("are we 3");
    // if let Some(local_desc) = peer_connection.local_description().await {
    //     let sdp = serde_json::to_string(&local_desc)?;

    //     Ok(sdp)
    // } else {
    //     println!("generate local_description failed!");
    //     Err(anyhow::Error::msg("generate local_description failed!"))
    // }
    
    Ok(())
}

fn prepare_api() -> Result<API, anyhow::Error> {
    // Create a MediaEngine object to configure the supported codec
    let mut m = MediaEngine::default();

    m.register_default_codecs()?;

    // Create a InterceptorRegistry. This is the user configurable RTP/RTCP Pipeline.
    // This provides NACKs, RTCP Reports and other features. If you use `webrtc.NewPeerConnection`
    // this is enabled by default. If you are manually managing You MUST create a InterceptorRegistry
    // for each PeerConnection.
    let mut registry = Registry::new();

    // Use the default set of Interceptors
    registry = register_default_interceptors(registry, &mut m)?;

    // Create the API object with the MediaEngine
    let api = APIBuilder::new()
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .build();

    Ok(api)
}

fn prepare_configuration() -> Result<RTCConfiguration, anyhow::Error> {
    // Prepare the configuration
    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };

    Ok(config)
}

// Loop through all the PCs and 
fn signal_peer_connections(pcs: HashMap<String, Arc<RTCPeerConnection>>) {
}
