<script lang="ts">
  import { onMount } from 'svelte'

  let pc: RTCPeerConnection
  let ws: WebSocket
  let uuid: string

  onMount(async () => {
    console.log('creating peer')
    await createPeerConnection()
    console.log('...done!')

    ws = new WebSocket("ws://localhost:8081")

    ws.onopen = e => {
      console.log("[open] Connection established")
    }

    ws.onmessage = async (event) => {
      console.log(`[message] Data received from server: ${event.data}`)
      let msg = JSON.parse(event.data)

      switch (msg.event) {
        case 'offer':
          uuid = msg.uuid
          let offer = JSON.parse(msg.data)
          if (!offer) {
            return console.log('failed to parse answer')
          }
          console.warn("Got this offer:", offer)

          console.log(pc.getSenders())
          await pc.setRemoteDescription(offer)

          const answer = await pc.createAnswer()
          console.warn('Sending answer.', answer)
          await pc.setLocalDescription(answer)

          ws.send(JSON.stringify({
            event: "answer",
            data: answer.sdp,
            uuid: msg.uuid
          }))

          return

          case 'candidate':
            console.log("candidate isssssssss:")
            console.log(msg)
            let candidate = JSON.parse(msg.data)
            if (!candidate) {
              return console.log('failed to parse candidate')
            }

            pc.addIceCandidate(candidate)
        }
    }
  })

  const createPeerConnection = async () => {
    let stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: false })

    pc = new RTCPeerConnection({
      iceServers: [
        {
          urls: 'stun:stun.l.google.com:19302'
        }
      ]
    })

    pc.ontrack = function (event) {
      console.log("Got a new track!", event)
      /* let el = document.getElementById('remote') as HTMLVideoElement */
      /* el.srcObject = event.streams[0] */
      /* el.autoplay = true */
      /* el.controls = true */

      if (event.track.kind === 'audio') {
        return
      }

      let el = document.createElement(event.track.kind) as HTMLVideoElement
      el.srcObject = event.streams[0]
      el.autoplay = true
      el.controls = true
      document.getElementById('remoteVideos').appendChild(el)

      event.track.onmute = function(event) {
        el.play()
      }

      event.streams[0].onremovetrack = ({track}) => {
        if (el.parentNode) {
          el.parentNode.removeChild(el)
        }
      }
    }

    pc.oniceconnectionstatechange = e => console.log(pc.iceConnectionState)

    pc.onicecandidate = e => {
      if (!e.candidate) {
        return
      }

      ws.send(JSON.stringify({event: 'candidate', uuid, data: JSON.stringify(e.candidate)}))
    }

    stream.getTracks().forEach(track => pc.addTrack(track, stream));
    let el = document.getElementById('local') as HTMLVideoElement
    el.srcObject = stream

    /* navigator.mediaDevices.getUserMedia({ video: true, audio: false }) */
    /*   .then(stream => { */
    /*     stream.getTracks().forEach(track => pc.addTrack(track, stream)); */
    /*     let el = document.getElementById('local') as HTMLVideoElement */
    /*     el.srcObject = stream */
    /*   }).catch(console.error) */
  }

  $: (window as any).pc = pc
</script>

<main>
  <div id="signalingContainer" style="display: none">
    <h2>Browser base64 Session Description</h2>
    <textarea id="localSessionDescription" readonly></textarea>

    <h2>Golang base64 Session Description</h2>
    <textarea id="remoteSessionDescription"></textarea>
  </div>

  <h2>Local Video</h2>
  <video id="local" width="160" height="120" autoplay muted></video>
  <h2>Remote Videos</h2>
  <div id="remoteVideos"></div> <br />

  <button class="createSessionButton" on:click={() => ""}> Publish a Broadcast </button>

  <h2>Logs</h2>
  <div id="logs"></div>
</main>

<style>
  h1, h2, h3, h4, h5, h6 {
    line-height: 1.2em;
    font-size: 1.2em;
  }
</style>
