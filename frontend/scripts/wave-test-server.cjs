const WebSocket = require('ws');
const fs = require('fs');
const path = require('path');

const PORT = 8080;
const WAVE_DIR = 'D:/Code/Monitor/deploy/standalone/data/data/wave/2026-06-28';

const wss = new WebSocket.Server({ port: PORT });

console.log(`WebSocket测试服务器启动在 ws://localhost:${PORT}`);

const LEAD_SAMPLE_RATES = {
  'I': 200,
  'II': 200,
  'III': 200,
  'aVR': 200,
  'aVL': 200,
  'aVF': 200,
  'IBP': 100,
  'Resp': 50,
};

function parseWaveFile(filePath) {
  try {
    const buffer = fs.readFileSync(filePath);
    const samples = [];
    
    for (let i = 0; i + 2 <= buffer.length; i += 2) {
      samples.push(buffer.readInt16LE(i));
    }
    
    return { samples };
  } catch (e) {
    console.error(`读取文件 ${filePath} 失败:`, e);
    return null;
  }
}

function getAllWaveFiles() {
  const files = [];
  try {
    const bedDirs = fs.readdirSync(WAVE_DIR);
    
    for (const bedDir of bedDirs) {
      if (!bedDir.startsWith('bed_')) continue;
      const bedNo = parseInt(bedDir.replace('bed_', ''));
      const bedPath = path.join(WAVE_DIR, bedDir);
      
      const waveFiles = fs.readdirSync(bedPath).filter(f => f.endsWith('.bin'));
      for (const waveFile of waveFiles) {
        const match = waveFile.match(/^([A-Za-z]+)_\d+\.bin$/);
        if (match) {
          files.push({
            bedNo,
            lead: match[1],
            filePath: path.join(bedPath, waveFile),
            fileName: waveFile
          });
        }
      }
    }
    console.log(`共找到 ${files.length} 个波形文件`);
  } catch (e) {
    console.error('读取波形目录失败:', e);
  }
  return files;
}

function groupFilesByLead(files) {
  const groups = {};
  for (const file of files) {
    const key = `${file.bedNo}_${file.lead}`;
    if (!groups[key]) {
      groups[key] = [];
    }
    groups[key].push(file);
  }
  console.log(`分组后共 ${Object.keys(groups).length} 个导联组`);
  return groups;
}

function generateWaveMessage(bedNo, lead, samples, sampleRate) {
  return JSON.stringify({
    msg_type: 'waveform',
    bed_no: bedNo,
    timestamp: Date.now(),
    data: [{
      device_id: `test_device_${bedNo}`,
      lead: lead,
      sample_time: new Date().toISOString(),
      sample_rate: sampleRate,
      samples: samples
    }]
  });
}

function generateVitalMessage(bedNo) {
  return JSON.stringify({
    msg_type: 'vital_signs',
    bed_no: bedNo,
    timestamp: Date.now(),
    data: {
      device_id: `test_device_${bedNo}`,
      heart_rate: 70 + Math.floor(Math.random() * 20),
      resp_rate: 15 + Math.floor(Math.random() * 10),
      spo2: 95 + Math.floor(Math.random() * 5),
      pi: (0.5 + Math.random() * 2).toFixed(1),
      pulse_rate: 70 + Math.floor(Math.random() * 20),
      nibp_sys: 110 + Math.floor(Math.random() * 20),
      nibp_dia: 70 + Math.floor(Math.random() * 15),
      nibp_map: 85 + Math.floor(Math.random() * 15),
      ibp_sys: 110 + Math.floor(Math.random() * 20),
      ibp_dia: 70 + Math.floor(Math.random() * 15),
      ibp_map: 85 + Math.floor(Math.random() * 15),
      device: {
        vendor: 'Comen',
        model: 'TestModel'
      }
    }
  });
}

const files = getAllWaveFiles();
const groups = groupFilesByLead(files);

const cachedData = {};
for (const [key, fileGroup] of Object.entries(groups)) {
  const [bedNoStr, lead] = key.split('_');
  const bedNo = parseInt(bedNoStr);
  const sampleRate = LEAD_SAMPLE_RATES[lead] || 200;
  
  if (fileGroup.length > 0) {
    const parsed = parseWaveFile(fileGroup[0].filePath);
    if (parsed && parsed.samples.length > 50) {
      cachedData[key] = { samples: parsed.samples, sampleRate };
      console.log(`缓存导联 ${key}: ${parsed.samples.length} 个采样点, 采样率 ${sampleRate}Hz`);
    }
  }
}

const groupKeys = Object.keys(cachedData);
console.log(`可发送的导联组: ${groupKeys.join(', ')}`);

wss.on('connection', (ws) => {
  console.log('客户端已连接');
  
  const subscriptions = new Set();
  let sendInterval = null;
  let sendCount = 0;
  
  ws.on('message', (message) => {
    try {
      const msg = JSON.parse(message);
      if (msg.action === 'subscribe') {
        subscriptions.add(msg.bed_no);
        console.log(`客户端订阅床位: ${msg.bed_no}`);
      } else if (msg.action === 'unsubscribe') {
        subscriptions.delete(msg.bed_no);
      } else if (msg.action === 'ping') {
        ws.send(JSON.stringify({ action: 'pong' }));
      }
    } catch (e) {
      console.error('消息解析失败:', e);
    }
  });
  
  ws.on('close', () => {
    console.log('客户端断开连接');
    if (sendInterval) {
      clearInterval(sendInterval);
      sendInterval = null;
    }
  });
  
  ws.on('error', (e) => {
    console.error('WebSocket错误:', e);
    if (sendInterval) {
      clearInterval(sendInterval);
      sendInterval = null;
    }
  });
  
  let currentGroupIndex = 0;
  
  sendInterval = setInterval(() => {
    try {
      if (ws.readyState !== WebSocket.OPEN) {
        console.log('WebSocket连接已关闭，停止发送');
        if (sendInterval) {
          clearInterval(sendInterval);
          sendInterval = null;
        }
        return;
      }
      
      if (groupKeys.length === 0) {
        return;
      }
      
      const key = groupKeys[currentGroupIndex];
      const [bedNoStr, lead] = key.split('_');
      const bedNo = parseInt(bedNoStr);
      
      if (!subscriptions.has(bedNo) && subscriptions.size > 0) {
        currentGroupIndex = (currentGroupIndex + 1) % groupKeys.length;
        return;
      }
      
      const data = cachedData[key];
      if (!data || data.samples.length < 50) {
        currentGroupIndex = (currentGroupIndex + 1) % groupKeys.length;
        return;
      }
      
      const batchSize = 50;
      const startIdx = Math.floor(Math.random() * (data.samples.length - batchSize));
      const batch = data.samples.slice(startIdx, startIdx + batchSize);
      
      ws.send(generateWaveMessage(bedNo, lead, batch, data.sampleRate));
      
      sendCount++;
      if (sendCount % 100 === 0) {
        console.log(`已发送 ${sendCount} 次消息`);
      }
      
      if (Math.random() < 0.1) {
        ws.send(generateVitalMessage(bedNo));
      }
      
      currentGroupIndex = (currentGroupIndex + 1) % groupKeys.length;
    } catch (e) {
      console.error('发送消息失败:', e);
    }
  }, 50);
  
  for (let i = 1; i <= 8; i++) {
    try {
      ws.send(generateVitalMessage(i));
    } catch (e) {
      console.error('发送初始生命体征失败:', e);
    }
  }
});

wss.on('error', (e) => {
  console.error('WebSocket服务器错误:', e);
});

console.log('等待客户端连接...');
console.log('可以在浏览器中打开前端页面查看波形效果');

process.on('uncaughtException', (err) => {
  console.error('未捕获的异常:', err);
});

process.on('unhandledRejection', (err) => {
  console.error('未处理的Promise拒绝:', err);
});

setInterval(() => {}, 1000);
