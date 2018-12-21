const log = (msg) => {
    let elem = document.getElementById("log");
    elem.innerHTML += msg + '<br />';
    elem.scrollTop += 1000;
};

let conn = null;

const connect = () => {
    disconnect();
    conn = new WebSocket('ws://' + window.location.host + '/ws');
    log('Connecting...');

    conn.onopen = function() {
        log('Connected.');
        update_ui();
    };

    conn.onmessage = function(e) {
        log('Received: ' + e.data);
    };

    conn.onclose = function() {
        log('Disconnected.');
        conn = null;
        update_ui();
    };
};

const disconnect = () => {
    if (conn == null) { return }

    log('Disconnecting...');
    conn.close();
    conn = null;
    update_ui();
};

const update_ui = () => {
    if (conn == null) {
        document.getElementById('status').innerText = 'disconnected';
        document.getElementById('connect').innerHTML = 'Connect';
    } else {
        document.getElementById('status').innerText = 'connected';
        document.getElementById('connect').innerHTML = 'Disconnect';
    }
}

document.getElementById('connect').onclick = () => {
    if (conn == null) {
        connect();
    } else {
        disconnect();
    }
    update_ui();
    return false;
};

document.getElementById('send').onclick = () => {
    let text_node = document.getElementById('text');
    let text = text_node.value;
    log('Sending: ' + text);
    conn.send(text);
    text_node.value = '';
    text_node.focus();
    return false;
};

document.getElementById('text').onkeyup = (e) => {
    if (e.keyCode === 13) {
        document.getElementById('send').click();
        return false;
    }
};
