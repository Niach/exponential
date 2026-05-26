import { useState } from "react"
import { CreditCard, Sparkles } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { authClient } from "@/lib/auth/client"

export function UpgradeDialog({
  open,
  onOpenChange,
  title,
  description,
  proProductId,
  businessProductId,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  description: string
  proProductId: string | null
  businessProductId: string | null
}) {
  const [loading, setLoading] = useState(false)

  const handleCheckout = async (productId: string) => {
    setLoading(true)
    try {
      const { data } = await (authClient as any).creem.createCheckout({
        productId,
        successUrl: window.location.href,
      })
      if (data?.url) window.location.href = data.url
    } catch (err) {
      console.error(`[billing] checkout failed:`, err)
    } finally {
      setLoading(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles className="size-4" />
            {title}
          </DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <p className="text-xs text-muted-foreground">
          Use code <span className="font-mono font-medium">FOUNDING</span> at
          checkout for 50% off forever.
        </p>
        <div className="flex gap-2 pt-2">
          {proProductId && (
            <Button
              onClick={() => handleCheckout(proProductId)}
              disabled={loading}
            >
              <CreditCard className="mr-1.5 size-3.5" />
              {loading ? `Loading...` : `Pro — $18/yr`}
            </Button>
          )}
          {businessProductId && (
            <Button
              variant="outline"
              onClick={() => handleCheckout(businessProductId)}
              disabled={loading}
            >
              {loading ? `Loading...` : `Business — $60/yr`}
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
