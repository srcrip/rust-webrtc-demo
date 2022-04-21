<script lang="ts">
  import { onMount } from 'svelte'

  let pc: RTCPeerConnection
  let ws = new WebSocket("ws://localhost:8081")

  onMount(() => {
    createPeerConnection()

    ws.onopen = e => {
      console.log("[open] Connection established")
    }

    ws.onmessage = async (event) => {
      console.log(`[message] Data received from server: ${event.data}`)
      let msg = JSON.parse(event.data)

      switch (msg.event) {
        case 'offer':
          let offer = JSON.parse(msg.data)
          if (!offer) {
            return console.log('failed to parse answer')
          }
          console.log("Got this offer:")
          console.log(offer)
          await pc.setRemoteDescription(new RTCSessionDescription(offer))
          //pc.createAnswer().then(answer => {
            //pc.setLocalDescription(answer)
            //ws.send(JSON.stringify({event: 'answer', data: JSON.stringify(answer)}))
          //})

          const answer = await pc.createAnswer()
          console.log('Sending answer.', answer)
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

  const createPeerConnection = () => {
    pc = new RTCPeerConnection({
      //iceServers: [
        //{
          //urls: 'stun:stun.l.google.com:19302'
        //}
      //]
    })

    pc.ontrack = function (event) {
      console.log("Got a new track!", event)
      let el = document.getElementById('remote') as HTMLVideoElement
      el.srcObject = event.streams[0]
      el.autoplay = true
      el.controls = true
    }

    pc.oniceconnectionstatechange = e => console.log(pc.iceConnectionState)

    pc.onicecandidate = e => {
      if (!e.candidate) {
        return
      }

      ws.send(JSON.stringify({event: 'candidate', data: JSON.stringify(e.candidate)}))
    }

    //pc.onicecandidate = event => {
      //if (event.candidate === null) {
        //let sdp = JSON.stringify(pc.localDescription)
        //console.log("Sending local sdp")
        ////ws.send(JSON.stringify({
          ////event: "answer",
          ////data: sdp,
          ////uuid: "whatever"
        ////}))
      //}
    //}

    navigator.mediaDevices.getUserMedia({ video: true, audio: false })
      .then(stream => {
        stream.getTracks().forEach(track => pc.addTrack(track, stream));
        let el = document.getElementById('local') as HTMLVideoElement
        el.srcObject = stream
      }).catch(console.error)
  }

  //const start = async (sdp: string) => {
    //try {
      //await pc.setRemoteDescription(new RTCSessionDescription(JSON.parse(sdp)))
    //} catch (e) {
      //alert(e)
    //}
  //}
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
  <h2>Remote Video</h2>
  <video id="remote" width="160" height="120" autoplay muted></video>

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
