import ReactDOM from 'react-dom/client'
import { App } from './components/App'
import 'maplibre-gl/dist/maplibre-gl.css'
import './styles.css'

// StrictMode disabled: Deck.gl WebGL device and Chart.js canvas refs don't
// survive the deliberate double-invoke that StrictMode uses in dev.
ReactDOM.createRoot(document.getElementById('root')!).render(<App />)
