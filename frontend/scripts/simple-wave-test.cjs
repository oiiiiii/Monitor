const WebSocket = require('ws');

const PORT = 8080;
const wss = new WebSocket.Server({ port: PORT });

console.log(`WebSocket测试服务器启动在 ws://localhost:${PORT}`);

let connectionCount = 0;

function generateECGSample(time) {
  const pWave = 0.05 * Math.sin(2 * Math.PI * 1.5 * time);
  const qrs = 0.5 * Math.sin(2 * Math.PI * 10 * time) * Math.exp(-Math.pow(time % 0.8 - 0.4, 2) / 0.02);
  const tWave = 0.15 * Math.sin(2 * Math.PI * 3 * time) * Math.exp(-Math.pow(time % 0.8 - 0.6, 2) / 0.03);
  const noise = (Math.random() - 0.5) * 0.02;
  return Math.round((pWave + qrs + tWave + noise) * 1000);
}

wss.on('connection', (ws) => {
  connectionCount++;
  console.log(`客户端已连接，总连接数: ${connectionCount}`);
  
  const subscriptions = new Set();
  let sendInterval = null;
  let time = 0;
  
  ws.on('error', (e) => {
    console.error('WebSocket连接错误:', e);
  });
  
  ws.on('message', (message) => {
    try {
      const msg = JSON.parse(message);
      if (msg.action === 'subscribe') {
        subscriptions.add(msg.bed_no);
        console.log(`客户端订阅床位: ${msg.bed_no}`);
      } else if (msg.action === 'ping') {
        ws.send(JSON.stringify({ action: 'pong' }));
      }
    } catch (e) {
      console.error('消息解析失败:', e);
    }
  });
  
  ws.on('close', () => {
    connectionCount--;
    console.log(`客户端断开连接，总连接数: ${connectionCount}`);
    if (sendInterval) {
      clearInterval(sendInterval);
    }
  });
  
  sendInterval = setInterval(() => {
    try {
      if (ws.readyState !== WebSocket.OPEN) {
        if (sendInterval) {
          clearInterval(sendInterval);
        }
        console.log('WebSocket连接已关闭，停止发送');
        return;
      }
      
      const ecgSamples = [];
      for (let i = 0; i < 20; i++) {
        const t = time + i * 0.005;
        ecgSamples.push(generateECGSample(t));
      }
      
      const msg = JSON.stringify({
        msg_type: 'waveform',
        bed_no: 1,
        timestamp: Date.now(),
        data: [{
          device_id: 'test_device_1',
          lead: 'II',
          sample_time: new Date().toISOString(),
          sample_rate: 200,
          samples: ecgSamples
        }]
      });
      
      ws.send(msg, (err) => {
        if (err) {
          console.error('发送数据失败:', err);
        }
      });
      
      time += 0.1;
      if (time % 2 < 0.1) {
        console.log(`发送数据中，当前时间: ${time.toFixed(1)}, 样本数: ${ecgSamples.length}`);
      }
    } catch (e) {
      console.error('发送数据时发生错误:', e);
    }
  }, 50);
  
  ws.send(JSON.stringify({
    msg_type: 'vital_signs',
    bed_no: 1,
    timestamp: Date.now(),
    data: {
      device_id: 'test_device_1',
      heart_rate: 75,
      resp_rate: 18,
      spo2: 98,
      pi: '1.2',
      pulse_rate: 75,
      device: {
        vendor: 'Comen',
        model: 'TestModel'
      }
    }
  }));
});

wss.on('error', (e) => {
  console.error('WebSocket服务器错误:', e);
});

console.log('等待客户端连接...');

setInterval(() => {
  console.log(`[${new Date().toISOString()}] 服务器运行中，当前连接数: ${connectionCount}`);
}, 10000);

process.on('exit', (code) => {
  console.log(`进程即将退出，退出码: ${code}`);
});

process.on('uncaughtException', (err) => {
  console.error('未捕获的异常:', err);
  process.exit(1);
});
