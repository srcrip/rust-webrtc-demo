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
use webrtc::rtp_transceiver::rtp_codec::{RTPCodecType, RTCRtpCodecCapability, RTCRtpCodecParameters};
use webrtc::rtp_transceiver::rtp_receiver::RTCRtpReceiver;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};
use webrtc::track::track_remote::TrackRemote;
use webrtc::Error;
use flume::{Sender, Receiver};
use std::collections::HashMap;
use sfu::media::Peer;

mod sfu;

#[derive(Debug, Clone)]
pub enum PeerChanCommand {
    SendIceCandidate {
        uuid: String,
        candidate: String,
    },
    ReceiveIceCandidate {
        uuid: String,
        candidate: String,
    },
    ReceiveOffer {
        uuid: String,
        sdp: String,
        tx: Sender<SocketMessage>
    },
    ReceiveAnswer {
        uuid: String,
        sdp: String,
    },
    OnTrack {
        uuid: String,
        track: Arc<TrackRemote>
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let new_conn_rx = sfu::signal::ws_sdp_signaler(8081).await;

    let (peer_chan_tx, peer_chan_rx) = flume::unbounded::<PeerChanCommand>();

    let tx_clone_1 = peer_chan_tx.clone();
    let tx_clone_2 = peer_chan_tx.clone();

    tokio::spawn(async move {
        println!("Creating peer channel listener.");
        sfu::media::handle_peer_connection_commands(peer_chan_rx, tx_clone_1.clone()).await.unwrap();
    });

    while let Ok((uuid, socket_tx, socket_rx)) = new_conn_rx.recv_async().await {
        handle_new_connection(&uuid, tx_clone_2.clone(), socket_tx, socket_rx).await.unwrap();
    };

    Ok(())
}

// Handler to spin off for every new connection.
async fn handle_new_connection(_uuid: &String, peer_chan_tx: Sender<PeerChanCommand>, socket_tx: Sender<SocketMessage>, socket_rx: Receiver<SocketMessage>) -> Result<()> {
    tokio::spawn(async move {
        println!("Handling a new connection.");
        while let Ok(signal) = socket_rx.recv_async().await {
            println!("Got a signal.");
            match signal {
                SocketMessage { event, uuid: id, data: sdp } if event == "offer" => {
                    peer_chan_tx.send(PeerChanCommand::ReceiveOffer {
                        uuid: id.to_owned(),
                        tx: socket_tx.clone(),
                        sdp
                    }).unwrap();
                },
                SocketMessage { event, uuid: id, data: sdp } if event == "answer" => {
                    peer_chan_tx.send(PeerChanCommand::ReceiveAnswer {
                        uuid: id.to_owned(),
                        sdp
                    }).unwrap();
                },
                SocketMessage { event, uuid: id, data: candidate } if event == "candidate" => {
                    peer_chan_tx.send(PeerChanCommand::ReceiveIceCandidate {
                        uuid: id.to_owned(),
                        candidate
                    }).unwrap();
                },
                _ => ()
            }
        };
    });

    Ok(())
}

// async fn create_peer(uuid: String, peer_chan_tx: Sender<PeerChanCommand>) -> Result<Arc<RTCPeerConnection>, anyhow::Error> {

//     println!("Creating the local peer");
//     // Create a new local RTCPeerConnection
//     let peer_connection = Arc::new(api.new_peer_connection(config).await?);

//     // Set a handler for when a new remote track starts, this handler copies inbound RTP packets,
//     // replaces the SSRC and sends them back
//     let pc = Arc::downgrade(&peer_connection);
//     let id = uuid.to_owned();
//     peer_connection
//         .on_track(Box::new(
//                 move |track: Option<Arc<TrackRemote>>, _receiver: Option<Arc<RTCRtpReceiver>>| {
//                     println!("Received a new track!!!");

//                     if let Some(track) = track {
//                         // Send a PLI on an interval so that the publisher is pushing a keyframe every rtcpPLIInterval
//                         // This is a temporary fix until we implement incoming RTCP events, then we would push a PLI only when a viewer requests it
//                         let media_ssrc = track.ssrc();
//                         let pc2 = pc.clone();
//                         tokio::spawn(async move {
//                             let mut result = Result::<usize>::Ok(0);
//                             while result.is_ok() {
//                                 let timeout = tokio::time::sleep(Duration::from_secs(3));
//                                 tokio::pin!(timeout);

//                                 tokio::select! {
//                                     _ = timeout.as_mut() =>{
//                                         if let Some(pc) = pc2.upgrade(){
//                                             result = pc.write_rtcp(&[Box::new(PictureLossIndication{
//                                                 sender_ssrc: 0,
//                                                 media_ssrc,
//                                             })]).await.map_err(Into::into);
//                                         } else {
//                                             break;
//                                         }
//                                     }
//                                 };
//                             }
//                         });

//                         let local_track_chan_tx = peer_chan_tx.clone();
//                         let mut id = id.to_owned();
//                         tokio::spawn(async move {
//                             // Create Track that we send video back to browser on
//                             let local_track = Arc::new(TrackLocalStaticRTP::new(
//                                     track.codec().await.capability,
//                                     "video".to_owned(),
//                                     "webrtc-rs".to_owned(),
//                             ));

//                             println!("");
//                             println!("Adding a new track.");
//                             println!("");
//                             local_track_chan_tx.send(
//                                 PeerChanCommand::AddTrack {
//                                     uuid: id,
//                                     track: Arc::clone(&local_track)
//                                 }
//                             ).unwrap();

//                             // Read RTP packets being sent to webrtc-rs
//                             while let Ok((rtp, _)) = track.read_rtp().await {
//                                 if let Err(err) = local_track.write_rtp(&rtp).await {
//                                     if Error::ErrClosedPipe != err {
//                                         print!("output track write_rtp got error: {} and break", err);
//                                         break;
//                                     } else {
//                                         print!("output track write_rtp got error: {}", err);
//                                     }
//                                 }
//                             }
//                         });
//                     }

//                     Box::pin(async {})
//                 },
//     ))
//         .await;

//     Ok(peer_connection)
// }
