<script lang="ts">
  import { onMount } from 'svelte'

  let pc: RTCPeerConnection
  let ws: WebSocket
  let uuid: string

  function randomId() {
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
      let r = Math.random() * 16 | 0, v = c == 'x' ? r : (r & 0x3 | 0x8)
      return v.toString(16)
    })
  }

  onMount(async () => {
    uuid = randomId()

    ws = new WebSocket("ws://localhost:8081")

    ws.onopen = async _e => {
      console.log("Connection established. Creating peer.")
      await createPeerConnection()
      connect()
    }

    ws.onmessage = async (event) => {
      console.log(`[message] Data received from server: ${event.data}`)
      let msg = JSON.parse(event.data)

      switch (msg.event) {
        case 'answer': {
          let answer = JSON.parse(msg.data)
          if (!answer) { return console.log('failed to parse offer') }

          await pc.setRemoteDescription(answer).then(() => pc = pc)

          return
        }
        case 'offer': {
          let offer = JSON.parse(msg.data)
          if (!offer) { return console.log('failed to parse offer') }

          console.warn("Got this offer:", offer)

          console.log(pc.getSenders())
          await pc.setRemoteDescription(offer).then(() => pc = pc)

          const answer = await pc.createAnswer()
          console.warn('Sending answer.', answer)
          await pc.setLocalDescription(answer).then(() => pc = pc)

          ws.send(JSON.stringify({
            event: "answer",
            data: answer.sdp,
            uuid: msg.uuid
          }))

          return
        }
        case 'candidate': {
          let candidate = JSON.parse(msg.data)
          if (!candidate) {
            return console.log('failed to parse candidate')
          }

          pc.addIceCandidate(candidate)
        }
      }
    }
  })

  const connect = async () => {
    const offer = await pc.createOffer()
    await pc.setLocalDescription(offer).then(() => pc = pc)

    ws.send(JSON.stringify({
      event: "offer",
      data: offer.sdp,
      uuid: uuid
    }))
  }

  const createPeerConnection = async () => {
    let stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: true })

    pc = new RTCPeerConnection({
      iceServers: [
        {
          urls: 'stun:stun.l.google.com:19302'
        }
      ]
    })

    pc.ontrack = function (event) {
      console.log("Got a new track!", event)

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

    pc.oniceconnectionstatechange = e => console.warn(pc.iceConnectionState)

    pc.onicecandidate = e => {
      if (!e.candidate) {
        return
      }

      ws.send(JSON.stringify({event: 'candidate', uuid, data: JSON.stringify(e.candidate)}))
    }

    stream.getTracks().forEach(track => pc.addTrack(track, stream));
    let el = document.getElementById('local') as HTMLVideoElement
    el.srcObject = stream
  }

  $: (window as any).pc = pc
</script>

<main>
  <h1>Peer ID: { uuid }</h1>
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

  <div class="grid">
    <div>
      <h4>Local SDP</h4>
      <pre>{pc?.localDescription?.sdp}</pre>
    </div>
    <div>
      <h4>Remote SDP</h4>
      <pre>{pc?.remoteDescription?.sdp}</pre>
    </div>
  </div>

  <h2>Logs</h2>
  <div id="logs"></div>
</main>

<style>
  pre {
    font-size: .9em;
  }

  .grid {
    display: grid;
  }

  h1, h2, h3, h4, h5, h6 {
    line-height: 1.2em;
    font-size: 1.2em;
  }
</style>
